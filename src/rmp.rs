use serde::{Serialize, Deserialize};
use failure::Fail;
use regex::Regex;
use futures::{TryFutureExt};

const SOLR_QUERY: &'static str = 
"https://solr-aws-elb-production.ratemyprofessors.com/solr/rmp/select\
?rows=200\
&wt=json\
&fq=schoolid_s:1077\
&defType=edismax\
&qf=teacherfirstname_t%5E2000+teacherlastname_t%5E2000+teacherfullname_t%5E2000+autosuggest\
&sort=score+desc\
&group=on\
&group.field=content_type_s\
&group.limit=-1\
&spellcheck=false\
&echoParams=none\
&q=";

const GRAPHQL_TOKEN_URL: &'static str =
"https://www.ratemyprofessors.com/ShowRatings.jsp?tid=10000";

const GRAPHQL_URL: &'static str =
"https://www.ratemyprofessors.com/graphql";

const GRAPHQL_QUERY: &'static str = 
r#"query RatingsListQuery(
    $id: ID!
    $courseFilter: String
) {
    node(id: $id) {
        __typename
        ... on Teacher {
            ...RatingsList_teacher_4pguUW
        }
        id
    }
}

fragment RatingsList_teacher_4pguUW on Teacher {
    id
    legacyId
    ...Rating_teacher
    ratings(courseFilter: $courseFilter) {
        edges {
            node {
              ...Rating_rating
              id
              __typename
            }
        }
    }
}

fragment Rating_teacher on Teacher {
    ...RatingFooter_teacher
}

fragment Rating_rating on Rating {
    comment
    ...RatingHeader_rating
    ...RatingValues_rating
    ...CourseMeta_rating
    ...RatingTags_rating
    ...RatingFooter_rating
}

fragment RatingHeader_rating on Rating {
    date
    class
    helpfulRating
    clarityRating
}

fragment RatingValues_rating on Rating {
    helpfulRating
    clarityRating
    difficultyRating
}

fragment CourseMeta_rating on Rating {
    courseType
    attendanceMandatory
    wouldTakeAgain
    grade
    textbookUse
}

fragment RatingTags_rating on Rating {
    ratingTags
}

fragment RatingFooter_rating on Rating {
    id
    comment
    legacyId
    thumbsUpTotal
    thumbsDownTotal
    thumbs {
        userId
        thumbsUp
        thumbsDown
        id
    }
}

fragment RatingFooter_teacher on Teacher {
    id
    legacyId
    lockStatus
}"#;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "ratemyprofessor.com broken")]
    RMP,
}

mod int_bool {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<bool, D::Error>
        where
            D: Deserializer<'de>,
    {
        match u8::deserialize(deserializer)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Unsigned(other as u64),
                &"zero or one",
            )),
        }
    }

    pub fn deserialize_opt<'de, D>(
        deserializer: D,
    ) -> Result<Option<bool>, D::Error>
        where
            D: Deserializer<'de>,
    {
        if let Ok(res) = Option::<u8>::deserialize(deserializer) {
            if let Some(i) = res {
                if i == 0 {
                    return Ok(Some(false));
                } else if i == 1 {
                    return Ok(Some(true));
                }
            }
            return Ok(None)
        }
        Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Unsigned(0),
            &"zero, one or null",
        ))
    }
}

mod rmp_date {
    use chrono::{DateTime, Utc, TimeZone};
    use serde::{self, Deserialize, Deserializer};

    const FORMAT: &'static str = "%Y-%m-%d %H:%M:%S";

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<DateTime<Utc>, D::Error>
        where
            D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let s = s.replace(" +0000 UTC", "");
        Utc.datetime_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

mod rmp_mandatory {
    use serde::{self, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<bool>, D::Error>
        where
            D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "mandatory" => Ok(Some(true)),
            "non mandatory" => Ok(Some(false)),
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLVariables {
    id: String,
    #[serde(rename = "courseFilter")]
    course_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLRequest {
    pub query: String,
    pub variables: GraphQLVariables,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfessorResponse {
    pub id: String,
    #[serde(rename = "averageratingscore_rf")]
    pub score: Option<f32>,
    #[serde(rename = "teacherfirstname_t")]
    pub first_name: String,
    #[serde(rename = "teacherlastname_t")]
    pub last_name: String,
    #[serde(rename = "teacherfullname_s")]
    pub full_name: String,
    #[serde(rename = "teacherdepartment_s")]
    pub department: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocListResponse {
    pub docs: Vec<ProfessorResponse>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupResponse {
    #[serde(rename = "groupValue")]
    pub group_name: String,
    #[serde(rename = "doclist")]
    pub doc_list: DocListResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerGroupedResponse {
    pub groups: Vec<GroupResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupedResponse {
    #[serde(rename = "content_type_s")]
    pub inner: InnerGroupedResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RMPResponse {
    pub grouped: GroupedResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thumb {
    #[serde(rename = "userId")]
    pub user_id: u32,
    #[serde(rename = "thumbsDown")]
    pub thumbs_down: u32,
    #[serde(rename = "thumbsUp")]
    pub thumbs_up: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rating {
    #[serde(rename = "attendanceMandatory", deserialize_with = "rmp_mandatory::deserialize")]
    pub attendance_mandatory: Option<bool>,
    #[serde(rename = "clarityRating")]
    pub clarity: u32,
    pub class: String,
    pub comment: String,
    #[serde(rename = "courseType")]
    pub course_type: Option<u32>,
    #[serde(deserialize_with = "rmp_date::deserialize")]
    pub date: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "difficultyRating")]
    pub difficulty: u32,
    pub grade: String,
    #[serde(rename = "helpfulRating")]
    pub helpful: u32,
    #[serde(rename = "ratingTags")]
    pub tags: String,
    #[serde(rename = "textbookUse")]
    pub textbook_use: Option<u32>,
    pub thumbs: Vec<Thumb>,
    #[serde(rename = "thumbsDownTotal")]
    pub thumbs_down: u32,
    #[serde(rename = "thumbsUpTotal")]
    pub thumbs_up: u32,
    #[serde(rename = "wouldTakeAgain", deserialize_with = "int_bool::deserialize_opt")]
    pub would_take_again: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerCommentsRatingsEdgesResponse {
    pub node: Rating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerCommentsRatingsResponse {
    pub edges: Vec<InnerCommentsRatingsEdgesResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerCommentsNodeResponse {
    pub ratings: InnerCommentsRatingsResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InnerCommentsDataResponse {
    pub node: InnerCommentsNodeResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentsResponse {
    pub data: InnerCommentsDataResponse,
}

pub async fn get_rmp_graphql_token() -> Result<String, Error> {
    let resp = reqwest::get(GRAPHQL_TOKEN_URL)
        .and_then(|r| async move { r.text().await }).map_err(|_| Error::RMP).await?;

    let re = Regex::new("\"REACT_APP_GRAPHQL_AUTH\":\"(.*?)\"").map_err(|_| Error::RMP)?;

    if let Some(cap) = re.captures_iter(resp.as_str()).next() {
        return Ok(cap[1].to_string());
    }

    Err(Error::RMP)
}


pub async fn search_professor(name: &str) -> Result<Vec<ProfessorResponse>, Error> {
    let resp = reqwest::get((SOLR_QUERY.to_owned() + name).as_str())
        .and_then(|r| async move { r.json::<RMPResponse>().await })
        .map_err(|e| { println!("get_professor_overview: error {}", e); Error::RMP}).await?;

    let grouped: Vec<GroupResponse> = resp.grouped.inner.groups;

    if let Some(teacher_group) = grouped.iter()
        .filter(|g| g.group_name == "TEACHER")
        .next() {
        return Ok(teacher_group
            .doc_list
            .docs
            .clone());
    }

    Ok(Vec::new())
}

pub async fn get_professor_comments(rmp_id: u32, course: Option<String>) -> Result<Vec<Rating>, Error> {
    let resp: CommentsResponse = reqwest::Client::builder().build().unwrap()
        .post(GRAPHQL_URL)
        .json(&GraphQLRequest {
            query: GRAPHQL_QUERY.to_owned(),
            variables: GraphQLVariables {
                id: base64::encode(format!("Teacher-{}", rmp_id).as_str()),
                course_filter: course
            }
        })
        .header(reqwest::header::AUTHORIZATION, "Basic dGVzdDp0ZXN0")
        .send()
        .and_then(|r| async move { r.json::<CommentsResponse>().await }).map_err(|e| { println!("{:?}", e); Error::RMP }).await?;

    let ratings = resp.data.node.ratings.edges.iter()
        .map(|r| r.node.clone())
        .collect();

    Ok(ratings)
}
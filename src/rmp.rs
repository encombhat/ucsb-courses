use serde::{Serialize, Deserialize};
use failure::Fail;
use regex::Regex;
use futures::TryFutureExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
            return Ok(None);
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
struct GraphQLVariables {
    id: String,
    #[serde(rename = "courseFilter")]
    course_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphQLRequest {
    pub query: String,
    pub variables: GraphQLVariables,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfessorResponse {
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
struct DocListResponse {
    pub docs: Vec<ProfessorResponse>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupResponse {
    #[serde(rename = "groupValue")]
    pub group_name: String,
    #[serde(rename = "doclist")]
    pub doc_list: DocListResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InnerGroupedResponse {
    pub groups: Vec<GroupResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupedResponse {
    #[serde(rename = "content_type_s")]
    pub inner: InnerGroupedResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RMPResponse {
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
struct InnerCommentsRatingsEdgesResponse {
    pub node: Rating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InnerCommentsRatingsResponse {
    pub edges: Vec<InnerCommentsRatingsEdgesResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InnerCommentsNodeResponse {
    pub ratings: InnerCommentsRatingsResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InnerCommentsDataResponse {
    pub node: InnerCommentsNodeResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CommentsResponse {
    pub data: InnerCommentsDataResponse,
}

#[derive(Debug, Clone)]
pub struct Score {
    pub quality: Option<f32>,
    pub quality_yr: Option<f32>,
}

#[derive(Clone)]
pub struct Professor {
    pub rmp_id: u32,

    pub score: Option<Score>,

    pub first_name: String,
    pub last_name: String,
    pub full_name: String,

    pub department: String,
}

struct ControllerData {
    rmp_graphql_token: Option<String>,
    name_id_map: HashMap<String, Vec<u32>>,
    id_professor_map: HashMap<u32, Arc<Mutex<Professor>>>,
}

pub struct Controller {
    data: Arc<Mutex<ControllerData>>,

    client: reqwest::Client,
}

impl Controller {
    pub fn new() -> Self {
        let controller_data = ControllerData {
            rmp_graphql_token: None,
            name_id_map: HashMap::new(),
            id_professor_map: HashMap::new(),
        };

        Controller {
            data: Arc::new(Mutex::new(controller_data)),
            client: reqwest::Client::default(),
        }
    }

    pub async fn graphql_token(&self) -> Result<String, Error> {
        {
            let data = self.data.lock().await;

            if let Some(token) = data.rmp_graphql_token.clone() {
                return Ok(token);
            }
        }

        let resp = self.client.get(GRAPHQL_TOKEN_URL)
            .send()
            .and_then(|r| async move { r.text().await }).map_err(|_| Error::RMP).await?;

        let re = Regex::new("\"REACT_APP_GRAPHQL_AUTH\":\"(.*?)\"").map_err(|_| Error::RMP)?;

        if let Some(cap) = re.captures_iter(resp.as_str()).next() {
            let token = cap[1].to_string();

            let mut data = self.data.lock().await;
            data.rmp_graphql_token = Some(token.clone());

            return Ok(token);
        }

        Err(Error::RMP)
    }

    pub async fn professor_overview(&self, name: String) -> Option<Arc<Mutex<Professor>>> {
        if let Some(pr) = self._name_to_professor(name).await {
            let professor_lock = pr.clone();
            let mut professor = professor_lock.lock().await;

            if let Some(score) = professor.score.clone() {
                return Some(pr);
            }

            if let Ok(resp) = self._professor_comments(professor.rmp_id, None).await {
                let (score, weight) = Self::_weighted_score(&resp, 157680000);
                let (score_yr, weight_yr) = Self::_weighted_score(&resp, 31536000);

                let professor_score = Score {
                    quality: if weight < 8.0 { None } else { Some(score / weight) },
                    quality_yr: if weight_yr < 2.0 { None } else { Some(score_yr / weight_yr) },
                };

                professor.score = Some(professor_score.clone());

                return Some(pr);
            }
        }

        None
    }

    pub async fn professor_comments(&self, name: String, course: Option<String>) -> Vec<Rating> {
        if let Some(pr) = self._name_to_professor(name).await {
            let professor = pr.lock().await;

            if let Ok(ratings) = self._professor_comments(professor.rmp_id, course).await {
                return ratings;
            }
        }

        Vec::new()
    }

    async fn _name_to_professor(&self, name: String) -> Option<Arc<Mutex<Professor>>> {
        let mut data = self.data.lock().await;

        let name = name.to_lowercase();
        let mut id_opt: Option<u32> = None;

        if let Some(ids) = data.name_id_map.get(name.as_str()) {
            id_opt = ids.get(0).cloned();
        } else {
            let res = self._search_professor(name.as_str()).await.ok()?;

            let ids: Vec<u32> = res.iter()
                .map(|r| &r.id)
                .map(|r| r.replace("teacher:", ""))
                .map(|r| r.parse::<u32>().ok())
                .filter_map(|r| r)
                .collect();

            id_opt = ids.get(0).cloned();

            data.name_id_map.insert(name, ids);

            for pr in res {
                if let Ok(id) = pr.id.replace("teacher:", "").parse::<u32>() {
                    if !data.id_professor_map.contains_key(&id) {
                        data.id_professor_map.insert(
                            id,
                            Arc::new(
                                Mutex::new(
                                    Professor {
                                        rmp_id: id,
                                        score: None,
                                        first_name: pr.first_name,
                                        last_name: pr.last_name,
                                        full_name: pr.full_name,
                                        department: pr.department,
                                    }
                                )
                            ),
                        );
                    }
                }
            }
        }

        if let Some(id) = id_opt {
            return data.id_professor_map.get(&id).cloned();
        }

        None
    }

    async fn _search_professor(&self, name: &str) -> Result<Vec<ProfessorResponse>, Error> {
        let resp = self.client.get((SOLR_QUERY.to_owned() + name).as_str())
            .send()
            .and_then(|r| async move { r.json::<RMPResponse>().await })
            .map_err(|e| {
                println!("get_professor_overview: error {}", e);
                Error::RMP
            }).await?;

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

    async fn _professor_comments(&self, rmp_id: u32, course: Option<String>) -> Result<Vec<Rating>, Error> {
        if let Ok(token) = self.graphql_token().await {
            let resp: CommentsResponse = self.client
                .post(GRAPHQL_URL)
                .json(&GraphQLRequest {
                    query: GRAPHQL_QUERY.to_owned(),
                    variables: GraphQLVariables {
                        id: base64::encode(format!("Teacher-{}", rmp_id).as_str()),
                        course_filter: course,
                    },
                })
                .header(reqwest::header::AUTHORIZATION, format!("Basic {}", token))
                .send()
                .and_then(|r| async move { r.json::<CommentsResponse>().await })
                .map_err(|e| {
                    println!("{:?}", e);
                    Error::RMP
                }).await?;

            let ratings = resp.data.node.ratings.edges.iter()
                .map(|r| r.node.clone())
                .collect();

            return Ok(ratings);
        }

        Err(Error::RMP)
    }

    fn _weighted_score(data: &Vec<Rating>, offset: u64) -> (f32, f32) {
        let mut quality_ratings_sum = 0.0;
        let mut total_weight = 0.0;

        let offsetted = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - offset;

        for r in data {
            if (r.date.timestamp() as u64) < offsetted {
                continue;
            }

            let quality = (r.helpful + r.clarity) as f32 / 2.0;

            let thumbs_weight = (r.thumbs_up + 1) as f32 / (r.thumbs_up + r.thumbs_down + 1) as f32;
            let time_weight = ((r.date.timestamp() as u64 - offsetted) as f64 / offset as f64) as f32;
            let quantity_weight = ((r.thumbs_up + r.thumbs_down) as f32 / 2.0).ln_1p() + 1.0;

            let avg_weight = thumbs_weight * time_weight * quantity_weight;

            total_weight += avg_weight;
            quality_ratings_sum += quality * avg_weight;
        }

        (quality_ratings_sum, total_weight)
    }
}

use actix_web::{web, App, HttpRequest, HttpServer, Responder};

use futures::{TryFutureExt};
use serde::{Serialize, Deserialize};
use regex::Regex;

mod rmp;

#[derive(Serialize, Deserialize)]
struct ProfessorResponse {
    pub score: Option<f32>,

    pub first_name: String,
    pub last_name: String,
    pub full_name: String,

    pub department: String,
}

async fn version(req: HttpRequest) -> impl Responder {
    "0.0.1"
}

async fn professor(req: HttpRequest) -> actix_web::Either<String, web::Json<Vec<ProfessorResponse>>> {
    if let Some(professor_name) = req.match_info().get("name") {
        let rmp_url = format!("https://solr-aws-elb-production.ratemyprofessors.com/solr/rmp/select\
        ?rows=200\
        &wt=json\
        &q={}\
        &fq=schoolid_s:1077\
        &defType=edismax\
        &qf=teacherfirstname_t%5E2000+teacherlastname_t%5E2000+teacherfullname_t%5E2000+autosuggest\
        &sort=score+desc\
        &group=on\
        &group.field=content_type_s\
        &group.limit=-1\
        &spellcheck=false\
        &echoParams=none", professor_name);

        match reqwest::get(rmp_url.as_str())
            .and_then(|r| async move { r.json::<rmp::RMPResponse>().await }).await {
            Ok(resp) => {
                let grouped: Vec<rmp::GroupResponse> = resp.grouped.inner.groups;

                if let Some(teacher_group) = grouped.iter()
                    .filter(|g| g.group_name == "TEACHER")
                    .next() {
                    let professors_response: Vec<ProfessorResponse> = teacher_group
                        .doc_list
                        .docs
                        .iter()
                        .map(|p| ProfessorResponse {
                            score: p.score,

                            first_name: p.first_name.clone(),
                            last_name: p.last_name.clone(),
                            full_name: p.full_name.clone(),

                            department: p.department.clone(),
                        })
                        .collect();

                    return actix_web::Either::B(web::Json(professors_response))
                }
            },
            Err(e) => println!("Error when querying {}: {:?}", professor_name, e),
        }
    }

    return actix_web::Either::A("{}".to_owned());
}

async fn rmp_graphql_token(req: HttpRequest) -> impl Responder {
    if let Ok(resp) = reqwest::get("https://www.ratemyprofessors.com/ShowRatings.jsp?tid=10000") // Who is this professor anyway?
        .and_then(|r| async move { r.text().await }).await {

        let re = Regex::new("\"REACT_APP_GRAPHQL_AUTH\":\"(.*?)\"").unwrap();

        if let Some(cap) = re.captures_iter(resp.as_str()).next() {
            return cap[1].to_string();
        }
    }

    "Ok".to_string()
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/version", web::get().to(version))
            .route("/r0/professor/{name}", web::get().to(professor))
            .route("/internal/rmp_graphql_token", web::get().to(rmp_graphql_token))
    })
        .bind("127.0.0.1:8000")?
        .run()
        .await
}

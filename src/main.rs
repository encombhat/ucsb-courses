use actix_web::{web, App, HttpRequest, HttpServer, Responder};

use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::sync::Arc;

mod rmp;

#[derive(Clone)]
struct Score {
    pub quality: Option<f32>,
    pub quality_yr: Option<f32>,
}

#[derive(Clone)]
struct Professor {
    pub rmp_id: u32,

    pub score: Option<Score>,

    pub first_name: String,
    pub last_name: String,
    pub full_name: String,

    pub department: String,
}

struct AppState {
    rmp_graphql_token: Mutex<Option<String>>,
    name_id_map: Mutex<HashMap<String, Vec<u32>>>,
    id_professor_map: Mutex<HashMap<u32, Arc<Mutex<Professor>>>>,
}

#[derive(Serialize, Deserialize)]
struct ProfessorResponse {
    pub rmp_id: u32,

    pub quality: Option<f32>,
    pub quality_yr: Option<f32>,

    pub first_name: String,
    pub last_name: String,
    pub full_name: String,

    pub department: String,
}

#[derive(Serialize, Deserialize)]
struct Comment {
    pub class: String,

    pub comment: String,

    pub grade: String,

    pub attendance_mandatory: Option<bool>,

    pub quality: f32,
    pub difficulty: f32,

    pub date: chrono::DateTime<chrono::Utc>,
}

async fn version() -> impl Responder {
    web::Json(json!({"version": "0.0.1"}))
}

async fn name_to_professor(name: String, data: web::Data<AppState>) -> Option<Arc<Mutex<Professor>>> {
    let name = name.to_lowercase();

    let mut id_opt: Option<u32> = None;

    let mut name_id_map = data.name_id_map.lock().await;
    let mut id_professor_map = data.id_professor_map.lock().await;

    if let Some(ids) = name_id_map.get(name.as_str()) {
        id_opt = ids.get(0).cloned();
    } else {
        let res = rmp::search_professor(name.as_str()).await.ok()?;

        let ids: Vec<u32> = res.iter()
            .map(|r| &r.id)
            .map(|r| r.replace("teacher:", ""))
            .map(|r| r.parse::<u32>().ok())
            .filter_map(|r| r)
            .collect();

        id_opt = ids.get(0).cloned();

        name_id_map.insert(name, ids);

        for pr in res {
            if let Ok(id) = pr.id.replace("teacher:", "").parse::<u32>() {
                if !id_professor_map.contains_key(&id) {
                    id_professor_map.insert(
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
        return id_professor_map.get(&id).cloned();
    }

    None
}

fn get_weighted_score(data: &Vec<rmp::Rating>, offset: u64) -> (f32, f32) {
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

async fn professor_overview(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    if let Some(pr) = name_to_professor(path.clone(), data).await {
        let mut professor = pr.lock().await;
        let p = professor.clone();

        if let Some(score) = professor.score.clone() {
            return actix_web::Either::A(web::Json(ProfessorResponse {
                rmp_id: p.rmp_id,
                quality: p.score.as_ref().map(|e| e.quality).flatten(),
                quality_yr: p.score.as_ref().map(|e| e.quality_yr).flatten(),
                first_name: p.first_name,
                last_name: p.last_name,
                full_name: p.full_name,
                department: p.department,
            }));
        }

        if let Ok(resp) = rmp::get_professor_comments(professor.rmp_id, None).await {
            let (score, weight) = get_weighted_score(&resp, 157680000);
            let (score_yr, weight_yr) = get_weighted_score(&resp, 31536000);

            let professor_score = Score {
                quality: if weight < 8.0 { None } else { Some(score / weight) },
                quality_yr: if weight_yr < 2.0 { None } else { Some(score_yr / weight_yr) },
            };

            professor.score = Some(professor_score.clone());

            return actix_web::Either::A(web::Json(ProfessorResponse {
                rmp_id: p.rmp_id,
                quality: professor_score.quality,
                quality_yr: professor_score.quality_yr,
                first_name: p.first_name,
                last_name: p.last_name,
                full_name: p.full_name,
                department: p.department,
            }));
        }
    }

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn professor_comments(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    if let Some(pr) = name_to_professor(path.clone(), data).await {
        let professor = pr.lock().await;

        if let Ok(resp) = rmp::get_professor_comments(professor.rmp_id, None).await {
            let comments: Vec<Comment> = resp.iter()
                .map(|r| Comment {
                    class: r.class.clone(),
                    comment: r.comment.replace("&quot;", r#"""#),
                    grade: r.grade.clone(),
                    attendance_mandatory: r.attendance_mandatory.clone(),
                    quality: (r.clarity + r.helpful) as f32 / 2.0,
                    difficulty: r.difficulty as f32,
                    date: r.date.clone(),
                })
                .collect();

            return actix_web::Either::A(web::Json(comments));
        }
    }

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn professor_course_comments(path: web::Path<(String, String)>, data: web::Data<AppState>) -> impl Responder {
    if let Some(pr) = name_to_professor(path.0.clone(), data).await {
        let professor = pr.lock().await;

        if let Ok(resp) = rmp::get_professor_comments(professor.rmp_id, Some(path.1.clone())).await {
            let comments: Vec<Comment> = resp.iter()
                .map(|r| Comment {
                    class: r.class.clone(),
                    comment: r.comment.replace("&quot;", "\""),
                    grade: r.grade.clone(),
                    attendance_mandatory: r.attendance_mandatory.clone(),
                    quality: (r.clarity + r.helpful) as f32 / 2.0,
                    difficulty: r.difficulty as f32,
                    date: r.date.clone(),
                })
                .collect();

            return actix_web::Either::A(web::Json(comments));
        }
    }

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn rmp_graphql_token(data: web::Data<AppState>) -> impl Responder {
    if let Some(token) = data.rmp_graphql_token.lock().await.clone() {
        return web::Json(json!({
            "token": token,
        }));
    }

    if let Ok(token) = rmp::get_rmp_graphql_token().await {
        let mut rmp_graphql_token = data.rmp_graphql_token.lock().await;
        *rmp_graphql_token = Some(token.clone());
        println!("/internal/rmp_graphql_token: cached {}", token);

        return web::Json(json!({
            "token": token,
        }));
    }

    web::Json(json!({
        "error": "RMP",
    }))
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState {
        rmp_graphql_token: Mutex::new(None),
        name_id_map: Mutex::new(HashMap::new()),
        id_professor_map: Mutex::new(HashMap::new()),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .route("/version", web::get().to(version))
            .route("/r0/professor/{name}/overview", web::get().to(professor_overview))
            .route("/r0/professor/{name}/comments", web::get().to(professor_comments))
            .route("/r0/professor/{name}/course/{course}/comments", web::get().to(professor_course_comments))
            .route("/internal/rmp_graphql_token", web::get().to(rmp_graphql_token))
    })
        .bind("localhost:8000")?
        .run()
        .await
}

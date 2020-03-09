use actix_web::{web, App, HttpRequest, HttpServer, Responder};

use futures::TryFutureExt;
use serde::{Serialize, Deserialize};
use regex::Regex;
use tokio::sync::Mutex;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use failure::_core::time::Duration;

mod rmp;

struct AppState {
    rmp_graphql_token: Mutex<Option<String>>,
}

#[derive(Serialize, Deserialize)]
struct ProfessorResponse {
    pub rmp_id: u32,

    pub score: Option<f32>,

    pub first_name: String,
    pub last_name: String,
    pub full_name: String,

    pub department: String,
}

#[derive(Serialize, Deserialize)]
struct ProfessorOverviewResponse {
    pub quality: Option<f32>,
    pub quality_yr: Option<f32>,
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

async fn version(req: HttpRequest) -> impl Responder {
    web::Json(json!({"version": "0.0.1"}))
}

async fn search_professor(path: web::Path<String>) -> impl Responder {
    if let Ok(ps) = rmp::search_professor(&*path).await {
        let professors: Vec<ProfessorResponse> = ps.iter()
            .map(|p| ProfessorResponse {
                rmp_id: p.id.replace("teacher:", "").parse::<u32>().unwrap_or(0),

                score: p.score,

                first_name: p.first_name.clone(),
                last_name: p.last_name.clone(),
                full_name: p.full_name.clone(),

                department: p.department.clone(),
            })
            .collect();

        return actix_web::Either::A(web::Json(professors));
    }

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
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
        quality_ratings_sum += (quality * avg_weight);
    }

    (quality_ratings_sum, total_weight)
}

async fn professor_overview(path: web::Path<u32>) -> impl Responder {
    if let Ok(resp) = rmp::get_professor_comments(*path, None).await {
        let (score, weight) = get_weighted_score(&resp, 157680000);
        let (score_yr, weight_yr) = get_weighted_score(&resp, 31536000);

        let overview = ProfessorOverviewResponse {
            quality: if weight < 8.0 { None } else { Some(score / weight) },
            quality_yr: if weight_yr < 2.0 { None } else { Some(score_yr / weight_yr) },
        };

        return actix_web::Either::A(web::Json(overview));
    }

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn professor_comments(path: web::Path<u32>) -> impl Responder {
    if let Ok(resp) = rmp::get_professor_comments(*path, None).await {
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

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn professor_course_comments(path: web::Path<(u32, String)>) -> impl Responder {
    if let Ok(resp) = rmp::get_professor_comments(path.0, Some(path.1.clone())).await {
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

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn rmp_graphql_token(req: HttpRequest, data: web::Data<AppState>) -> impl Responder {
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
    HttpServer::new(|| {
        App::new()
            .data(AppState {
                rmp_graphql_token: Mutex::new(None),
            })
            .route("/version", web::get().to(version))
            .route("/r0/professor/search/{name}", web::get().to(search_professor))
            .route("/r0/professor/{id}/overview", web::get().to(professor_overview))
            .route("/r0/professor/{id}/comments", web::get().to(professor_comments))
            .route("/r0/professor/{id}/course/{course}/comments", web::get().to(professor_course_comments))
            .route("/internal/rmp_graphql_token", web::get().to(rmp_graphql_token))
    })
        .bind("localhost:8000")?
        .run()
        .await
}

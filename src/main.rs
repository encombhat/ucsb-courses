use actix_web::{web, App, HttpRequest, HttpServer, Responder};

use serde::{Serialize, Deserialize};
use tokio::sync::Mutex;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::sync::Arc;

mod rmp;

struct AppState {
    rmp_controller: rmp::Controller,
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

async fn professor_overview(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    if let Some(pr) = data.rmp_controller.professor_overview(path.clone()).await {
        let professor = pr.lock().await;
        let p: rmp::Professor = professor.clone();

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

    actix_web::Either::B(web::Json(json!({"error": "RMP"})))
}

async fn professor_comments(path: web::Path<String>, data: web::Data<AppState>) -> impl Responder {
    let comments: Vec<Comment> = data.rmp_controller.professor_comments(path.clone(), None).await
        .iter()
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

    return web::Json(comments);
}

async fn professor_course_comments(path: web::Path<(String, String)>, data: web::Data<AppState>) -> impl Responder {
    let comments: Vec<Comment> = data.rmp_controller.professor_comments(path.0.clone(), Some(path.1.clone())).await
        .iter()
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

    return web::Json(comments);
}

async fn rmp_graphql_token(data: web::Data<AppState>) -> impl Responder {
    if let Ok(token) = data.rmp_controller.graphql_token().await {
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
        rmp_controller: rmp::Controller::new(),
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

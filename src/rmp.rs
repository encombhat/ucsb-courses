use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfessorResponse {
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
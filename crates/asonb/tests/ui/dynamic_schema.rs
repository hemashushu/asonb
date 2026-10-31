#[derive(serde::Serialize, asonb::Schema)]
struct Dynamic {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<i32>,
}
fn main() {}
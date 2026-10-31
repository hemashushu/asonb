#[derive(serde::Serialize, asonb::Schema)]
struct Flatten {
    #[serde(flatten)]
    fields: std::collections::BTreeMap<String, i32>,
}
fn main() {}
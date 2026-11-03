use asonb::Schema;

#[derive(Schema)]
struct Unit;

#[derive(Schema)]
struct Width(u32);

#[derive(Schema)]
struct RGB(u8, u8, u8);

#[derive(Schema)]
struct EmptyTuple();

#[derive(Schema)]
#[serde(transparent)]
struct Transparent(u32);

#[derive(Schema)]
enum EmptyVariant {
    Empty(),
}

#[derive(Schema)]
enum SkippedVariant {
    One(u32, #[serde(skip)] u32),
}

fn main() {}
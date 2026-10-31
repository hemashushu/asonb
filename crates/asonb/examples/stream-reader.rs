use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
struct Object {
    id: i32,
    name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::io::stdin().lock();
    let values = asonb::list_stream_reader::<Object, _>(input)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        values,
        vec![
            Object {
                id: 11,
                name: "foo".to_owned()
            },
            Object {
                id: 13,
                name: "bar".to_owned()
            }
        ]
    );
    eprintln!("Streaming deserialization successful!");
    Ok(())
}

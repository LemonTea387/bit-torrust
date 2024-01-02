use serde_json::Map;

fn decode_bencoded_value(encoded_value: &str) -> (serde_json::Value, &str) {
    // If encoded_value starts with a digit, it's a number
    match encoded_value.chars().next().unwrap() {
        x if x.is_ascii_digit() => {
            // Example: "5:hello" -> "hello"
            let (string, rem) = bendecode_s(encoded_value);
            (serde_json::Value::String(string), rem)
        }
        'i' => {
            let (val, rem) = bendecode_i(encoded_value);
            (serde_json::Value::Number(val.into()), rem)
        }
        'l' => {
            let (list, rem) = bendecode_l(encoded_value);
            (serde_json::Value::Array(list), rem)
        }
        'd' => {
            let (dict, rem) = bendecode_d(encoded_value);
            (serde_json::Value::Object(dict), rem)
        }
        _ => panic!("Unhandled encoded value: {}", encoded_value),
    }
}

fn bendecode_s(encoded_value: &str) -> (String, &str) {
    let colon_index = encoded_value.find(':').unwrap();
    let number_string = &encoded_value[..colon_index];
    let number = number_string.parse::<u32>().unwrap();
    let string = &encoded_value[colon_index + 1..colon_index + 1 + number as usize];
    (
        string.to_string(),
        &encoded_value[colon_index + 1 + number as usize..],
    )
}

fn bendecode_i(encoded_value: &str) -> (i64, &str) {
    // NOTE: Skip 'i'
    let ending_index = encoded_value.find('e').unwrap();
    let i_string = &encoded_value[1..ending_index];
    let number = i_string.parse::<i64>().unwrap();
    (number, &encoded_value[ending_index + 1..])
}

fn bendecode_l(encoded_value: &str) -> (Vec<serde_json::Value>, &str) {
    let mut list = Vec::new();

    let mut rem = encoded_value.split_at(1).1;
    while !rem.is_empty() && !rem.starts_with('e') {
        let (val, returned) = decode_bencoded_value(rem);
        list.push(val);
        rem = returned;
    }
    (list, rem.strip_prefix('e').unwrap())
}

fn bendecode_d(encoded_value: &str) -> (Map<String, serde_json::Value>, &str) {
    // We know that they must be strings.
    let mut dict = Map::new();
    let mut rem = encoded_value.split_at(1).1;
    while !rem.is_empty() && !rem.starts_with('e') {
        let (key, returned) = bendecode_s(rem);
        let (val, returned) = decode_bencoded_value(returned);
        dict.insert(key, val);
        rem = returned;
    }
    (dict, rem.strip_prefix('e').unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let (decoded_value, _) = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value);
    } else {
        eprintln!("unknown command: {}", args[1])
    }
}

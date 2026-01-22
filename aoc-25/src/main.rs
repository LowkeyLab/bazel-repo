use code;
use runfiles::Runfiles;

fn main() {
    let r = Runfiles::create().unwrap();

    let path = r.rlocation("_main/aoc-25/src/data.txt").unwrap();

    let input = std::fs::read_to_string(path).unwrap();

    let rotations = code::Code::parse_from_str(&input);
    let code = code::Code::new();
    let result = code.get_code(rotations.into_iter());

    println!("The code is: {}", result);
}

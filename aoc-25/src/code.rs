#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Rotation {
    Left(u32),
    Right(u32),
}

#[derive(Debug, Default)]
pub struct Code {}

impl Code {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_code<I>(&self, rotations: I) -> u32
    where
        I: Iterator<Item = Rotation>,
    {
        let mut count: u32 = 0;
        let mut position: i32 = 50;

        for rotation in rotations {
            match rotation {
                Rotation::Left(steps) => {
                    position = position - steps as i32;

                    while position < 0 {
                        position += 100;
                    }

                    if position == 0 {
                        count += 1;
                    }
                }
                Rotation::Right(steps) => {
                    position = position + steps as i32;

                    while position >= 100 {
                        position -= 100;
                    }

                    if position == 0 {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    pub fn parse_from_str(input: &str) -> Vec<Rotation> {
        input
            .lines()
            .filter_map(|line| {
                let parts = line.split_at(1);
                let direction = parts.0;
                let steps: u32 = parts.1.parse().ok()?;
                match direction {
                    "L" => Some(Rotation::Left(steps)),
                    "R" => Some(Rotation::Right(steps)),
                    _ => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_get_code() {
        let code = Code::new();
        let rotations = vec![
            Rotation::Left(68),
            Rotation::Left(30),
            Rotation::Right(48),
            Rotation::Left(5),
            Rotation::Right(60),
            Rotation::Left(55),
            Rotation::Left(1),
            Rotation::Left(99),
            Rotation::Right(14),
            Rotation::Left(82),
        ];
        let result = code.get_code(rotations.into_iter());
        assert_eq!(result, 3);
    }

    #[test]
    fn test_parse_from_str() {
        let input = "L68\nL30\nR48\nL5\nR60\nL55\nL1\nL99\nR14\nL82";
        let expected = vec![
            Rotation::Left(68),
            Rotation::Left(30),
            Rotation::Right(48),
            Rotation::Left(5),
            Rotation::Right(60),
            Rotation::Left(55),
            Rotation::Left(1),
            Rotation::Left(99),
            Rotation::Right(14),
            Rotation::Left(82),
        ];
        let result = Code::parse_from_str(input);
        assert_eq!(result, expected);
    }
}

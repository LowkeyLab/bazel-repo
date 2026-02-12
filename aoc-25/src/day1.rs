#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Rotation {
    Left(u32),
    Right(u32),
}

#[derive(Debug)]
pub struct Code {
    position: u32,
}

#[derive(Debug, PartialEq, Eq, Hash, Default)]
pub struct CodeResult {
    pub passes: u32,
    pub final_position: u32,
}

impl std::fmt::Display for CodeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Final Position: {}, Passes over zero: {}",
            self.final_position, self.passes
        )
    }
}

impl Default for Code {
    fn default() -> Self {
        Self { position: 50 }
    }
}

impl Code {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_code<I>(&self, rotations: I) -> CodeResult
    where
        I: Iterator<Item = Rotation>,
    {
        let mut count: u32 = 0;
        let mut position: u32 = self.position + 10000000;

        for rotation in rotations {
            match rotation {
                Rotation::Left(steps) => {
                    let mut steps = steps;
                    while steps > 0 {
                        position -= 1;
                        if position % 100 == 0 {
                            count += 1;
                        }
                        steps -= 1;
                    }
                }
                Rotation::Right(steps) => {
                    let mut steps = steps;
                    while steps > 0 {
                        position += 1;
                        if position % 100 == 0 {
                            count += 1;
                        }
                        steps -= 1;
                    }
                }
            }
        }

        CodeResult {
            passes: count,
            final_position: position % 100,
        }
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
    use test_case::test_case;

    #[test]
    fn test_initial_position() {
        let code = Code::new();
        assert_eq!(code.position, 50);
    }

    #[test_case(50, Rotation::Left(68), 82, 1; "50 -> L68 ends at 82")]
    #[test_case(52, Rotation::Right(48), 0, 1; "52 -> R48 ends at zero")]
    #[test_case(0, Rotation::Left(1), 99, 0; "0 -> L1 ends at 99")]
    #[test_case(99, Rotation::Right(2), 1, 1; "99 -> R2 ends at 1")]
    #[test_case(0, Rotation::Right(200), 0, 2; "0 -> R200 ends at 0")]
    #[test_case(0, Rotation::Left(200), 0, 2; "0 -> L200 ends at 0")]
    #[test_case(50, Rotation::Right(1000), 50, 10; "50 -> R1000 ends at 50")]
    #[test_case(50, Rotation::Right(50), 0, 1; "50 -> R50 ends at 0")]
    #[test_case(0, Rotation::Right(100), 0, 1; "0 -> R100 ends at 0")]
    fn passes_zero(
        initial_position: u32,
        rotation: Rotation,
        expected_position: u32,
        expected_count: u32,
    ) {
        let expected = CodeResult {
            passes: expected_count,
            final_position: expected_position,
        };
        let sut = Code {
            position: initial_position,
        };
        let rotations = vec![rotation];
        let result = sut.get_code(rotations.into_iter());
        assert_eq!(result, expected);
    }

    #[test_case(vec![Rotation::Right(50)], CodeResult { passes: 1, final_position: 0 }; "L60 from 50 ends at 90 with 1 pass")]
    #[test_case(vec![Rotation::Right(50), Rotation::Left(100)], CodeResult { passes: 2, final_position: 0 }; "R50 then L100 from 50 ends at 50 with 2 passes")]
    fn multiple_rotations_test(rotations: Vec<Rotation>, expected: CodeResult) {
        let sut = Code::new();
        let result = sut.get_code(rotations.into_iter());
        assert_eq!(result, expected);
    }

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
        assert_eq!(
            result,
            CodeResult {
                passes: 6,
                final_position: 32
            }
        );
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

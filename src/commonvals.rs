pub struct CommonValue {
    pub name: &'static str,
    pub description: &'static str,
    pub value: &'static str,
}

pub enum ValueCategory {
    LetterUS,
    A4,
}

impl ValueCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LetterUS  => "Letter (US)",
            Self::A4        => "A4 (ISO 216)",
        }
    }

    pub fn get_values(&self) -> Vec<CommonValue> {
        match self {
            ValueCategory::LetterUS => vec![
                CommonValue {
                    name: "Width mm",
                    description: "Width of Letter (US) paper in millimeters",
                    value: "215.9",
                },
                CommonValue {
                    name: "Height mm",
                    description: "Height of Letter (US) paper in millimeters",
                    value: "279.4",
                }
            ],
            ValueCategory::A4 => vec![
                CommonValue {
                    name: "Width mm",
                    description: "Width of A4 (ISO 216) paper in millimeters",
                    value: "210",
                },
                CommonValue {
                    name: "Height mm",
                    description: "Height of A4 (ISO 216) paper in millimeters",
                    value: "297",
                }
            ],
        }
    }
}
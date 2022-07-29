use std::fmt::Display;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde::de::{self, Visitor};

pub use UrbitVersion::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum UrbitVersion {
    UrbitV1_0,
    UrbitV1_1,
    UrbitV1_2,
    UrbitV1_3,
    UrbitV1_4,
    UrbitV1_5,
    UrbitV1_6,
    UrbitV1_7,
    UrbitV1_8,
    UrbitV1_9,
}

impl Default for UrbitVersion {
    fn default() -> Self {
        UrbitV1_9
    }
}

impl TryFrom<f32> for UrbitVersion {
    type Error = anyhow::Error;

    fn try_from(v: f32) -> Result<Self> {
        (v as f64).try_into()
    }
}

impl TryFrom<f64> for UrbitVersion {
    type Error = anyhow::Error;

    fn try_from(v: f64) -> Result<Self> {
             if v == 1.0 { Ok(UrbitV1_0) }
        else if v == 1.1 { Ok(UrbitV1_1) }
        else if v == 1.2 { Ok(UrbitV1_2) }
        else if v == 1.3 { Ok(UrbitV1_3) }
        else if v == 1.4 { Ok(UrbitV1_4) }
        else if v == 1.5 { Ok(UrbitV1_5) }
        else if v == 1.6 { Ok(UrbitV1_6) }
        else if v == 1.7 { Ok(UrbitV1_7) }
        else if v == 1.8 { Ok(UrbitV1_8) }
        else if v == 1.9 { Ok(UrbitV1_9) }
        else { bail!("invalid urbit version: {}", v) }
    }
}

impl TryFrom<&str> for UrbitVersion {
    type Error = anyhow::Error;

    fn try_from(v: &str) -> Result<Self> {
        match v {
            "1.0" | "v1.0" => Ok(UrbitV1_0),
            "1.1" | "v1.1" => Ok(UrbitV1_1),
            "1.2" | "v1.2" => Ok(UrbitV1_2),
            "1.3" | "v1.3" => Ok(UrbitV1_3),
            "1.4" | "v1.4" => Ok(UrbitV1_4),
            "1.5" | "v1.5" => Ok(UrbitV1_5),
            "1.6" | "v1.6" => Ok(UrbitV1_6),
            "1.7" | "v1.7" => Ok(UrbitV1_7),
            "1.8" | "v1.8" => Ok(UrbitV1_8),
            "1.9" | "v1.9" => Ok(UrbitV1_9),
            _ => bail!("invalid urbit version: {}", v)
        }
    }
}

impl Into<String> for UrbitVersion {
    fn into(self) -> String {
        match self {
            UrbitV1_0 => "v1.1".to_owned(),
            UrbitV1_1 => "v1.1".to_owned(),
            UrbitV1_2 => "v1.2".to_owned(),
            UrbitV1_3 => "v1.3".to_owned(),
            UrbitV1_4 => "v1.4".to_owned(),
            UrbitV1_5 => "v1.5".to_owned(),
            UrbitV1_6 => "v1.6".to_owned(),
            UrbitV1_7 => "v1.7".to_owned(),
            UrbitV1_8 => "v1.8".to_owned(),
            UrbitV1_9 => "v1.9".to_owned(),
        }
    }
}

impl Into<f32> for UrbitVersion {
    fn into(self) -> f32 {
        match self {
            UrbitV1_0 => 1.1,
            UrbitV1_1 => 1.1,
            UrbitV1_2 => 1.2,
            UrbitV1_3 => 1.3,
            UrbitV1_4 => 1.4,
            UrbitV1_5 => 1.5,
            UrbitV1_6 => 1.6,
            UrbitV1_7 => 1.7,
            UrbitV1_8 => 1.8,
            UrbitV1_9 => 1.9,
        }
    }
}

impl Into<f64> for UrbitVersion {
    fn into(self) -> f64 {
        let result: f32 = self.into();
        result as f64
    }
}

impl Display for UrbitVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: String = (*self).into();
        f.write_str(&s)
    }
}

impl Serialize for UrbitVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: serde::Serializer
    {
        serializer.serialize_str(&format!("{}", *self))
    }
}

struct UrbitVersionVisitor;

impl<'de> Visitor<'de> for UrbitVersionVisitor {
    type Value = UrbitVersion;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string \"vMAJOR.MINOR\" or a fractional numeral MAJOR.MINOR")
    }

    fn visit_f32<E: de::Error>(self, v: f32) -> std::result::Result<Self::Value, E> {
        v.try_into().map_err(E::custom)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
        v.try_into().map_err(E::custom)
    }

    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
        v.try_into().map_err(E::custom)
    }

    fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> std::result::Result<Self::Value, E> {
        v.try_into().map_err(E::custom)
    }

    fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Self::Value, E> {
        let vref: &str = &v;
        vref.try_into().map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for UrbitVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(UrbitVersionVisitor)
    }
}
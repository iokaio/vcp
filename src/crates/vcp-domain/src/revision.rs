// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! counters {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);
        impl $name {
            pub const ZERO: Self = Self(0);
            pub const fn new(value: u64) -> Self { Self(value) }
            pub const fn get(self) -> u64 { self.0 }
            pub fn next(self) -> crate::Result<Self> {
                self.0.checked_add(1).map(Self).ok_or(crate::Error::Overflow)
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
                s.serialize_str(&self.0.to_string())
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                let value: u64 = text.parse().map_err(serde::de::Error::custom)?;
                if value.to_string() != text { return Err(serde::de::Error::custom("counter must be a canonical unsigned decimal string")); }
                Ok(Self(value))
            }
        }
    )+};
}

counters!(
    Revision,
    SessionSeq,
    MemorySeq,
    SteeringRevision,
    PolicyRevision,
    AuthorityRevision,
    DeletionEpoch,
    OwnerEpoch,
    Watermark,
    ByteCount,
    Timestamp
);

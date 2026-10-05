// SPDX-License-Identifier: Apache-2.0
//! Copy-on-write ownership for canonical state components; wire bytes are unchanged.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

/// Shares an immutable state component across readers. Mutation detaches the
/// value first, so a prior snapshot never observes a candidate or failed write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SharedStateValue<T>(Arc<T>);
impl<T> From<T> for SharedStateValue<T> {
    fn from(value: T) -> Self {
        Self(Arc::new(value))
    }
}
impl<A, T: FromIterator<A>> FromIterator<A> for SharedStateValue<T> {
    fn from_iter<I: IntoIterator<Item = A>>(iter: I) -> Self {
        T::from_iter(iter).into()
    }
}
impl<T> Deref for SharedStateValue<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: Clone> DerefMut for SharedStateValue<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}
impl<T: Serialize> Serialize for SharedStateValue<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_ref().serialize(serializer)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for SharedStateValue<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(T::deserialize(deserializer)?.into())
    }
}
impl<'a, T> IntoIterator for &'a SharedStateValue<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.0.as_ref().into_iter()
    }
}
impl<'a, T: Clone> IntoIterator for &'a mut SharedStateValue<T>
where
    &'a mut T: IntoIterator,
{
    type Item = <&'a mut T as IntoIterator>::Item;
    type IntoIter = <&'a mut T as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        Arc::make_mut(&mut self.0).into_iter()
    }
}
impl<T: Clone + IntoIterator> IntoIterator for SharedStateValue<T> {
    type Item = T::Item;
    type IntoIter = T::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        Arc::unwrap_or_clone(self.0).into_iter()
    }
}

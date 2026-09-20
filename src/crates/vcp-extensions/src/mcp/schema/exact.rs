// SPDX-License-Identifier: Apache-2.0
//! Private exact numeric and lexical helpers for mcp-schema/2.
//! Exact bounded decimal operations and a scoped JSON reader into existing Value.
use serde_json::{Map, Number, Value};
use std::{cmp::Ordering, collections::BTreeMap, str::FromStr};

pub const NUMBER_BYTES: usize = 256;
pub const SIGNIFICANT_DIGITS: usize = 128;
pub const EXPONENT: i32 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Syntax,
    Bound,
    Duplicate,
    Reserved,
    InvalidDivisor,
    Schema,
    Arguments,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub depth: usize,
    pub nodes: usize,
    pub numeric_digits: usize,
    pub work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024,
            depth: 16,
            nodes: 4096,
            numeric_digits: 16 * 1024,
            work: 1_000_000,
        }
    }
}
/// One shared allowance across parsing and exact predicates. No reset per branch.
#[derive(Debug)]
pub struct Budget {
    work: usize,
    digits: usize,
}
impl Budget {
    pub fn new(work: usize, digits: usize) -> Self {
        Self { work, digits }
    }
    pub(super) fn charge(&mut self, amount: usize) -> Result<()> {
        self.work = self.work.checked_sub(amount).ok_or(Error::Bound)?;
        Ok(())
    }
    fn digits(&mut self, amount: usize) -> Result<()> {
        self.digits = self.digits.checked_sub(amount).ok_or(Error::Bound)?;
        Ok(())
    }
}

/// Nonzero coefficient has neither leading nor trailing zeroes; zero is [0], e0.
/// Debug intentionally omits external payload values.
#[derive(Clone, PartialEq, Eq)]
pub struct Decimal {
    negative: bool,
    coefficient: Vec<u8>,
    exponent: i32,
}
impl std::fmt::Debug for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decimal")
            .field("digits", &self.coefficient.len())
            .finish_non_exhaustive()
    }
}
impl Decimal {
    pub fn parse(token: &str, budget: &mut Budget) -> Result<Self> {
        if token.is_empty() || token.len() > NUMBER_BYTES {
            return Err(Error::Bound);
        }
        budget.charge(token.len())?;
        budget.digits(token.bytes().filter(u8::is_ascii_digit).count())?;
        let bytes = token.as_bytes();
        let negative = bytes[0] == b'-';
        let mut at = usize::from(negative);
        let integer_start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == integer_start || (at - integer_start > 1 && bytes[integer_start] == b'0') {
            return Err(Error::Syntax);
        }
        let integer_end = at;
        let mut fraction = &bytes[at..at];
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if at == start {
                return Err(Error::Syntax);
            }
            fraction = &bytes[start..at];
        }
        let mut explicit_exponent = 0i32;
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            let minus = bytes.get(at) == Some(&b'-');
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            let start = at;
            while let Some(digit) = bytes.get(at).filter(|b| b.is_ascii_digit()) {
                explicit_exponent = explicit_exponent
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(i32::from(*digit - b'0')))
                    .ok_or(Error::Bound)?;
                if explicit_exponent > EXPONENT {
                    return Err(Error::Bound);
                }
                at += 1;
            }
            if at == start {
                return Err(Error::Syntax);
            }
            if minus {
                explicit_exponent = -explicit_exponent;
            }
        }
        if at != bytes.len() {
            return Err(Error::Syntax);
        }
        let mut coefficient: Vec<u8> = bytes[integer_start..integer_end]
            .iter()
            .chain(fraction)
            .map(|b| *b - b'0')
            .collect();
        let first = coefficient.iter().position(|d| *d != 0);
        let Some(first) = first else {
            return Ok(Self {
                negative: false,
                coefficient: vec![0],
                exponent: 0,
            });
        };
        coefficient.drain(..first);
        let mut exponent = explicit_exponent - fraction.len() as i32;
        while coefficient.last() == Some(&0) {
            coefficient.pop();
            exponent += 1;
        }
        if coefficient.len() > SIGNIFICANT_DIGITS || exponent.abs() > EXPONENT {
            return Err(Error::Bound);
        }
        Ok(Self {
            negative,
            coefficient,
            exponent,
        })
    }
    pub fn is_zero(&self) -> bool {
        self.coefficient == [0]
    }
    pub fn is_integer(&self) -> bool {
        self.is_zero() || self.exponent >= 0
    }
    /// Keeps old exact integer encodings; all other forms have one coefficient/e.
    pub fn token(&self) -> String {
        let mut output = String::new();
        if self.negative {
            output.push('-');
        }
        for digit in &self.coefficient {
            output.push(char::from(b'0' + *digit));
        }
        if self.exponent >= 0
            && self.coefficient.len() + self.exponent as usize <= SIGNIFICANT_DIGITS
        {
            output.extend(std::iter::repeat_n('0', self.exponent as usize));
        } else if self.exponent != 0 {
            output.push('e');
            if self.exponent > 0 {
                output.push('+');
            }
            output.push_str(&self.exponent.to_string());
        }
        output
    }
    pub fn number(&self) -> Result<Number> {
        Number::from_str(&self.token()).map_err(|_| Error::Syntax)
    }
    pub fn compare(&self, other: &Self, budget: &mut Budget) -> Result<Ordering> {
        budget.charge(1)?;
        if self.is_zero() || other.is_zero() {
            return Ok(match (self.is_zero(), other.is_zero()) {
                (true, true) => Ordering::Equal,
                (true, false) => {
                    if other.negative {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                }
                _ => {
                    if self.negative {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                }
            });
        }
        if self.negative != other.negative {
            return Ok(if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            });
        }
        let left_magnitude = self.coefficient.len() as i32 + self.exponent;
        let right_magnitude = other.coefficient.len() as i32 + other.exponent;
        let mut order = left_magnitude.cmp(&right_magnitude);
        if order == Ordering::Equal {
            for i in 0..self.coefficient.len().max(other.coefficient.len()) {
                budget.charge(1)?;
                order = self
                    .coefficient
                    .get(i)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&other.coefficient.get(i).copied().unwrap_or(0));
                if order != Ordering::Equal {
                    break;
                }
            }
        }
        Ok(if self.negative {
            order.reverse()
        } else {
            order
        })
    }
    /// JSON Schema multipleOf uses a strictly positive divisor and exact remainder.
    pub fn multiple_of(&self, divisor: &Self, budget: &mut Budget) -> Result<bool> {
        budget.charge(1)?;
        if divisor.negative || divisor.is_zero() {
            return Err(Error::InvalidDivisor);
        }
        if self.is_zero() {
            return Ok(true);
        }
        let delta = self.exponent - divisor.exponent;
        let appended = delta.max(0) as usize;
        let denominator_length = divisor.coefficient.len() + (-delta).max(0) as usize;
        budget.charge(denominator_length + self.coefficient.len() + appended)?;
        let mut denominator = divisor.coefficient.clone();
        if delta < 0 {
            denominator.resize(denominator_length, 0);
        }
        let mut remainder = Vec::new();
        for digit in self
            .coefficient
            .iter()
            .copied()
            .chain(std::iter::repeat_n(0, appended))
        {
            budget.charge(1)?;
            remainder.push(digit);
            trim(&mut remainder);
            while unsigned_cmp(&remainder, &denominator, budget)? != Ordering::Less {
                subtract(&mut remainder, &denominator, budget)?;
            }
        }
        Ok(remainder == [0])
    }
}
fn trim(value: &mut Vec<u8>) {
    let first = value
        .iter()
        .position(|v| *v != 0)
        .unwrap_or(value.len().saturating_sub(1));
    value.drain(..first);
}
fn unsigned_cmp(left: &[u8], right: &[u8], budget: &mut Budget) -> Result<Ordering> {
    budget.charge(1)?;
    if left.len() != right.len() {
        return Ok(left.len().cmp(&right.len()));
    }
    for (a, b) in left.iter().zip(right) {
        budget.charge(1)?;
        let order = a.cmp(b);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}
fn subtract(left: &mut Vec<u8>, right: &[u8], budget: &mut Budget) -> Result<()> {
    budget.charge(left.len())?;
    let mut borrow = 0i16;
    for offset in 0..left.len() {
        let index = left.len() - 1 - offset;
        let sub = right
            .len()
            .checked_sub(1 + offset)
            .map_or(0, |i| i16::from(right[i]));
        let digit = i16::from(left[index]) - sub - borrow;
        left[index] = if digit < 0 {
            (digit + 10) as u8
        } else {
            digit as u8
        };
        borrow = i16::from(digit < 0);
    }
    if borrow != 0 {
        return Err(Error::Syntax);
    }
    trim(left);
    Ok(())
}

/// Exact Value with sorted object construction. No authority or schema claim.
pub struct Parsed {
    value: Value,
    canonical: Vec<u8>,
    budget: Budget,
}
impl Parsed {
    #[cfg(test)]
    pub fn value(&self) -> &Value {
        &self.value
    }
    #[cfg(test)]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn into_parts(self) -> (Value, Vec<u8>, Budget) {
        (self.value, self.canonical, self.budget)
    }
}
pub fn parse(bytes: &[u8], limits: Limits) -> Result<Parsed> {
    if limits.bytes == 0
        || limits.bytes > 1024 * 1024
        || bytes.len() > limits.bytes
        || limits.depth > 32
        || limits.nodes == 0
        || limits.nodes > 65536
        || limits.work > 10_000_000
        || limits.numeric_digits > 65536
    {
        return Err(Error::Bound);
    }
    let mut reader = Reader {
        bytes,
        at: 0,
        nodes: limits.nodes,
        depth: limits.depth,
        budget: Budget::new(limits.work, limits.numeric_digits),
    };
    let value = reader.value(0)?;
    reader.space()?;
    if reader.at != bytes.len() {
        return Err(Error::Syntax);
    }
    // Integer normalization can expand a short exponent token. Bound output
    // before extending its allocation, rather than checking after to_vec.
    let mut output = Output {
        bytes: Vec::new(),
        limit: limits.bytes,
        budget: &mut reader.budget,
    };
    serde_json::to_writer(&mut output, &value).map_err(|_| Error::Bound)?;
    let canonical = output.bytes;
    Ok(Parsed {
        value,
        canonical,
        budget: reader.budget,
    })
}
struct Output<'a> {
    bytes: Vec<u8>,
    limit: usize,
    budget: &'a mut Budget,
}
impl std::io::Write for Output<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("numeric prototype output bound"));
        }
        self.budget
            .charge(bytes.len())
            .map_err(|_| std::io::Error::other("numeric prototype work bound"))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    nodes: usize,
    depth: usize,
    budget: Budget,
}
impl Reader<'_> {
    fn space(&mut self) -> Result<()> {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\r' | b'\n' | b'\t'))
        {
            self.budget.charge(1)?;
            self.at += 1;
        }
        Ok(())
    }
    fn punctuation(&mut self, expected: u8) -> Result<()> {
        self.space()?;
        self.budget.charge(1)?;
        if self.bytes.get(self.at) != Some(&expected) {
            return Err(Error::Syntax);
        }
        self.at += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String> {
        self.space()?;
        let start = self.at;
        self.punctuation(b'"')?;
        loop {
            self.budget.charge(1)?;
            let byte = *self.bytes.get(self.at).ok_or(Error::Syntax)?;
            self.at += 1;
            match byte {
                b'"' => {
                    return serde_json::from_slice::<String>(&self.bytes[start..self.at])
                        .map_err(|_| Error::Syntax)
                }
                b'\\' => {
                    self.budget.charge(1)?;
                    if self.bytes.get(self.at).is_none() {
                        return Err(Error::Syntax);
                    }
                    self.at += 1;
                }
                _ => (),
            }
        }
    }
    fn value(&mut self, depth: usize) -> Result<Value> {
        if depth > self.depth || self.nodes == 0 {
            return Err(Error::Bound);
        }
        self.nodes -= 1;
        self.budget.charge(1)?;
        self.space()?;
        let byte = *self.bytes.get(self.at).ok_or(Error::Syntax)?;
        match byte {
            b'"' => self.string().map(Value::String),
            b'{' => {
                self.at += 1;
                self.space()?;
                let mut values = BTreeMap::new();
                if self.bytes.get(self.at) != Some(&b'}') {
                    loop {
                        let key = self.string()?;
                        if matches!(
                            key.as_str(),
                            "$serde_json::private::Number" | "$serde_json::private::RawValue"
                        ) {
                            return Err(Error::Reserved);
                        }
                        if values.contains_key(&key) {
                            return Err(Error::Duplicate);
                        }
                        self.punctuation(b':')?;
                        let value = self.value(depth + 1)?;
                        values.insert(key, value);
                        self.space()?;
                        if self.bytes.get(self.at) != Some(&b',') {
                            break;
                        }
                        self.at += 1;
                    }
                }
                self.punctuation(b'}')?;
                Ok(Value::Object(values.into_iter().collect::<Map<_, _>>()))
            }
            b'[' => {
                self.at += 1;
                self.space()?;
                let mut values = Vec::new();
                if self.bytes.get(self.at) != Some(&b']') {
                    loop {
                        values.push(self.value(depth + 1)?);
                        self.space()?;
                        if self.bytes.get(self.at) != Some(&b',') {
                            break;
                        }
                        self.at += 1;
                    }
                }
                self.punctuation(b']')?;
                Ok(Value::Array(values))
            }
            b't' | b'f' | b'n' => {
                let (literal, value): (&[u8], _) = match byte {
                    b't' => (b"true", Value::Bool(true)),
                    b'f' => (b"false", Value::Bool(false)),
                    _ => (b"null", Value::Null),
                };
                self.budget.charge(literal.len())?;
                if self.bytes.get(self.at..self.at + literal.len()) != Some(literal) {
                    return Err(Error::Syntax);
                }
                self.at += literal.len();
                Ok(value)
            }
            b'-' | b'0'..=b'9' => {
                let start = self.at;
                while self.bytes.get(self.at).is_some_and(|b| {
                    b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E')
                }) {
                    if self.at - start >= NUMBER_BYTES {
                        return Err(Error::Bound);
                    }
                    self.budget.charge(1)?;
                    self.at += 1;
                }
                let token =
                    std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| Error::Syntax)?;
                Ok(Value::Number(
                    Decimal::parse(token, &mut self.budget)?.number()?,
                ))
            }
            _ => Err(Error::Syntax),
        }
    }
}

#[cfg(test)]
#[path = "exact_tests.rs"]
mod tests;

/*
PORTED FROM C# ValvePak

This code is licenced under MIT
// The MIT License
//
// Copyright (c) 2006-2008 TinyVine Software Limited.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.
Portions of this software are Copyright of Simone Chiaretta
Portions of this software are Copyright of Nate Kohari
Portions of this software are Copyright of Alex Henderson
*/

#[derive(Debug)]
pub struct BerDecodeError {
  pub _message: String,
  pub _position: u32
}

pub struct RSAParameters {
  pub modulus: Box<[u8]>,
  pub exponent: Box<[u8]>
}

pub struct AsnKeyParser {
  parser: AsnParser
}

impl AsnKeyParser {
  pub fn new(contents: &[u8]) -> Self {
    Self {
      parser: AsnParser::new(contents)
    }
  }

  pub fn trim_leading_zero(values: &[u8]) -> Box<[u8]> {
    (if values[0] == 0x00 && values.len() > 1 {
      values[1..values.len()].to_vec()
    } else {
      values.to_vec()
    }).into_boxed_slice()
  }

  pub fn equal_oid(first: &[u8], second: &[u8]) -> bool {
    if first.len() != second.len() {
      return false;
    }
    !first.iter().enumerate().any(|(i, t)| *t != second[i])
  }

  pub fn parse_rsa_public_key(&mut self) -> Result<RSAParameters, BerDecodeError> {
    // Checkpoint
    let mut position = self.parser.current_position();

    // Ignore Sequence - PublicKeyInfo
    let length = self.parser.next_sequence()?;
    if length != self.parser.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect Sequence Size. Specified {}, Remaining: {}", length, self.parser.remaining_bytes()),
        _position: position
      });
    }

    // Checkpoint
    position = self.parser.current_position();

    // Ignore Sequence - AlgorithmIdentifier
    let length = self.parser.next_sequence()?;
    if length != self.parser.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect AlgorithmIdentifier Size. Specified {}, Remaining: {}", length, self.parser.remaining_bytes()),
        _position: position
      });
    }

    // Checkpoint
    position = self.parser.current_position();
    // Grab the OID
    let value = self.parser.next_oid()?;
    let oid: [u8; 9] = [ 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01 ];
    if !AsnKeyParser::equal_oid(&value, &oid) {
      return Err(BerDecodeError {
        _message: "Expected OID 1.2.840.113549.1.1.1".to_string(),
        _position: position
      });
    }

    // Optional Parameters
    if self.parser.is_next_null() {
      let _ = self.parser.next_null()?;
    } else {
      // Gracefully skip the optional data
      let _ = self.parser.next()?;
    }

    // Checkpoint
    position = self.parser.current_position();

    // Ignore BitString - PublicKey
    let length = self.parser.next_bit_string()?;
    if length != self.parser.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect PublicKey Size. Specified {}, Remaining: {}", length, self.parser.remaining_bytes()),
        _position: position
      });
    }

    // Checkpoint
    position = self.parser.current_position();

    // Ignore sequence - RsaPublicKey
    let length = self.parser.next_sequence()?;
    if length != self.parser.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect RsaPublicKey Size. Specified {}, Remaining: {}", length, self.parser.remaining_bytes()),
        _position: position
      });
    }

    Ok(RSAParameters {
      modulus: AsnKeyParser::trim_leading_zero(&self.parser.next_integer()?),
      exponent: AsnKeyParser::trim_leading_zero(&self.parser.next_integer()?)
    })
  }
}

pub struct AsnParser {
  initial_count: u32,
  octets: Vec<u8>
}

impl AsnParser {
  pub fn new(values: &[u8]) -> Self {
    let mut octets = Vec::<u8>::with_capacity(values.len());
    octets.extend_from_slice(values);
    let initial_count = octets.len() as u32;
    Self {
      octets,
      initial_count
    }
  }

  pub fn current_position(&self) -> u32 {
    self.initial_count - self.octets.len() as u32
  }

  pub fn remaining_bytes(&self) -> u32 {
    self.octets.len() as u32
  }

  pub fn len(&mut self) -> Result<u32, BerDecodeError> {
    let mut length = 0u32;
    let position = self.current_position();

    let b = self.get_next_octet()?;
    if b == (b & 0x7f) {
      return Ok(b as u32);
    }

    let mut i = b & 0x7f;
    if i > 4 {
      return Err(BerDecodeError {
        _message: format!("Invalid Length Encoding. Length uses {} octets", i),
        _position: position
      });
    }

    while i != 0 {
      i -= 1;

      length <<= 8;
      length |= self.get_next_octet()? as u32;
    }
    Ok(length)
  }

  pub fn next(&mut self) -> Result<Box<[u8]>, BerDecodeError> {
    let position = self.current_position();

    let _ = self.get_next_octet()?;

    let length = self.len()?;
    if length > self.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect Size. Specified {}, Remaining: {}", length, self.remaining_bytes()),
        _position: position
      });
    }
    self.get_octets(length)
  }

  fn get_next_octet(&mut self) -> Result<u8, BerDecodeError> {
    let position = self.current_position();

    if self.remaining_bytes() == 0 {
      return Err(BerDecodeError {
        _message: "Incorrect Size. Specified: 1, Remaining: 0".to_string(),
        _position: position
      });
    }

    Ok(self.get_octets(1)?[0])
  }

  fn get_octets(&mut self, octet_count: u32) -> Result<Box<[u8]>, BerDecodeError> {
    let position = self.current_position();

    if octet_count > self.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect Size. Specified: {}, Remaining: {}", octet_count, self.remaining_bytes()),
        _position: position
      });
    }

    let values: Vec<u8> = self.octets.drain(0..octet_count as usize).collect();
    Ok(values.into_boxed_slice())
  }

  pub fn is_next_null(&self) -> bool {
    self.octets[0] == 0x05
  }

  pub fn next_null(&mut self) -> Result<u32, BerDecodeError> {
    let position = self.current_position();

    let mut b = self.get_next_octet()?;
    if b != 0x05 {
      return Err(BerDecodeError {
        _message: format!("Expected Null. Specified Identifier: {}", b),
        _position: position
      });
    }

    b = self.get_next_octet()?;
    if b != 0x00 {
      return Err(BerDecodeError {
        _message: format!("Null has non-zero size. Size: {}", b),
        _position: position
      });
    }

    Ok(0)
  }

  pub fn next_sequence(&mut self) -> Result<u32, BerDecodeError> {
    let position = self.current_position();
    let b = self.get_next_octet()?;
    if b != 0x30 {
      return Err(BerDecodeError {
        _message: format!("Expected Sequence. Specified Identifier: {}", b),
        _position: position
      });
    }

    let length = self.len()?;
    if length > self.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect Sequence Size. Specified: {}, Remaining: {}", length, self.remaining_bytes()),
        _position: position
      });
    }
    Ok(length)
  }

  pub fn next_bit_string(&mut self) -> Result<u32, BerDecodeError> {
    let position = self.current_position();
    let mut b = self.get_next_octet()?;
    if b != 0x03 {
      return Err(BerDecodeError {
        _message: format!("Expected Sequence. Specified Identifier: {}", b),
        _position: position
      });
    }

    let mut length = self.len()?;

    // We need to consume unused bits, which is the first
    // octet of the remaining values
    b = self.octets.remove(0);
    length -= 1;

    if b != 0x00 {
      return Err(BerDecodeError {
        _message: "The first octet of BitString must be 0".to_string(),
        _position: position
      });
    }

    Ok(length)
  }

  pub fn next_integer(&mut self) -> Result<Box<[u8]>, BerDecodeError> {
    let position = self.current_position();
    let b = self.get_next_octet()?;
    if b != 0x02 {
      return Err(BerDecodeError {
        _message: format!("Expected Sequence. Specified Identifier: {}", b),
        _position: position
      });
    }

    let length = self.len()?;
    if length > self.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect Integer Size. Specified: {}, Remaining: {}", length, self.remaining_bytes()),
        _position: position
      });
    }

    self.get_octets(length)
  }

  pub fn next_oid(&mut self) -> Result<Box<[u8]>, BerDecodeError> {
    let position = self.current_position();
    let b = self.get_next_octet()?;
    if b != 0x06 {
      return Err(BerDecodeError {
        _message: format!("Expected Sequence. Specified Identifier: {}", b),
        _position: position
      });
    }

    let length = self.len()?;
    if length > self.remaining_bytes() {
      return Err(BerDecodeError {
        _message: format!("Incorrect Object Identifier Size. Specified: {}, Remaining: {}", length, self.remaining_bytes()),
        _position: position
      });
    }

    let values: Box<[u8]> = self.octets.drain(0..length as usize).collect();
    Ok(values)
  }
}

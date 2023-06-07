use anyhow::{anyhow, Result};
use regex::Regex;
use std::fs::{self, read};

#[macro_use]
extern crate lazy_static;

#[derive(Debug)]
pub struct PvlReader {
    content: String,
    pos: usize,
}

#[derive(Debug)]
pub enum Error {
    Eof,
    Syntax(String),
    CommentIsntComment,
    Programming(String),
    InvalidType,
    ValueTypeParseError,
}

#[derive(Debug)]
pub enum Symbol {
    Pointer(String),
    Key(String),
    Group,
    Object,
    BlankLine,
    ValueLineContinuation,
}

impl Symbol {
    pub fn value(&self) -> Option<String> {
        match self {
            Symbol::Pointer(value) => Some(value.to_owned()),
            Symbol::Key(value) => Some(value.to_owned()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ValueUnits {
    Celcius,
    Farenheit,
    Degrees,
    Radians,
    Milliseconds,
    Seconds,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ValueType {
    Undetermined,
    Array,
    String,
    Float,
    Integer,
    Bool,
    Flag, // A string but not wrapped in quotes
    BitMask,
}

#[derive(Debug)]
pub struct Value {
    value_raw: String,
    value_type: ValueType,
}

lazy_static! {
    static ref BOOL_DETERMINATE: Regex = Regex::new("^\"(TRUE|FALSE)\"$").unwrap();
    static ref STRING_DETERMINATE: Regex = Regex::new("^\".*\"$").unwrap();
    static ref ARRAY_DETERMINATE: Regex = Regex::new("^\\(.*\\)$").unwrap();
    static ref FLOAT_DETERMINATE: Regex = Regex::new("^-*[0-9]+\\.[0-9][ ]*").unwrap();
    static ref INTEGER_DETERMINATE: Regex = Regex::new("^-*[0-9]+[^#]+[ ]*").unwrap();
    static ref FLAG_DETERMINATE: Regex = Regex::new("^[a-zA-Z_]+[a-zA-Z0-9]+$").unwrap();
    static ref BITMASK_DETERMINATE: Regex = Regex::new("^[1-8]*#[0-1]+#$").unwrap();
}

// I think you'll get a lot of value out of this sorta thing for parsing libraries.
/// Implements the miscellanous parsing functions for Value
#[macro_export]
macro_rules! impl_parse_fn {
    ($fn_name:ident, $type:ty, $value_type:expr) => {
        // the $values just get swapped in.
        pub fn $fn_name(&self) -> Result<$type, Error> {
            if self.value_type != $value_type {
                return Err(Error::InvalidType);
            }
            match self.value_raw.parse::<$type>() {
                Ok(v) => Ok(v),
                Err(_) => Err(Error::ValueTypeParseError),
            }
        }
    };
}

impl Value {
    impl_parse_fn!(parse_f32, f32, ValueType::Float);
    impl_parse_fn!(parse_f64, f64, ValueType::Float);
    impl_parse_fn!(parse_u8, u8, ValueType::Integer);
    impl_parse_fn!(parse_u16, u16, ValueType::Integer);
    impl_parse_fn!(parse_u32, u32, ValueType::Integer);
    impl_parse_fn!(parse_u64, u64, ValueType::Integer);
    impl_parse_fn!(parse_i8, i8, ValueType::Integer);
    impl_parse_fn!(parse_i16, i16, ValueType::Integer);
    impl_parse_fn!(parse_i32, i32, ValueType::Integer);
    impl_parse_fn!(parse_i64, i64, ValueType::Integer);
    impl_parse_fn!(parse_bool, bool, ValueType::Bool);
    impl_parse_fn!(parse_string, String, ValueType::String);
    impl_parse_fn!(parse_flag, String, ValueType::Flag);
    // parse_array requires more macro expertise than I can provide if you want the one, and only one macro to cover everything.

    pub fn new(value_raw: &str) -> Self {
        Value {
            value_raw: value_raw.to_owned(),
            value_type: Value::determine_type(value_raw),
        }
    }

    fn determine_type(value_raw: &str) -> ValueType {
        if BOOL_DETERMINATE.is_match(value_raw) {
            ValueType::Bool
        } else if STRING_DETERMINATE.is_match(value_raw) {
            ValueType::String
        } else if ARRAY_DETERMINATE.is_match(value_raw) {
            ValueType::Array
        } else if FLOAT_DETERMINATE.is_match(value_raw) {
            ValueType::Float
        } else if INTEGER_DETERMINATE.is_match(value_raw) {
            ValueType::Integer
        } else if FLAG_DETERMINATE.is_match(value_raw) {
            ValueType::Flag
        } else if BITMASK_DETERMINATE.is_match(value_raw) {
            ValueType::BitMask
        } else {
            ValueType::Undetermined
        }
    }

    pub fn parse_array(&self) -> Result<Vec<Value>, Error> {
        if self.value_type != ValueType::Array {
            Err(Error::InvalidType)
        } else {
            Ok(self.value_raw.split(",").map(|v| Value::new(&v)).collect())
        }
    }
}

#[derive(Debug)]
pub struct KeyValuePairRaw {
    key: Symbol,
    value: Value,
}

impl PvlReader {
    pub fn new(content: &str) -> Self {
        PvlReader {
            content: content.to_owned(),
            pos: 0,
        }
    }

    pub fn char_at(&self, indx: usize) -> Result<char, Error> {
        if indx >= self.content.len() {
            Err(Error::Eof)
        } else {
            //Ok(self.content.chars().nth(indx).unwrap()) // Slow but correct(er)
            Ok(self.content.as_bytes()[indx] as char) // WAY faster, but won't work for non 8-bit text files
        }
    }

    pub fn char_at_pos_plus_n(&self, indx: usize) -> Result<char, Error> {
        if self.pos + indx >= self.content.len() {
            Err(Error::Eof)
        } else {
            //Ok(self.content.chars().nth(indx).unwrap()) // Slow but correct(er)
            Ok(self.content.as_bytes()[self.pos + indx] as char) // WAY faster, but won't work for non 8-bit text files
        }
    }

    pub fn current_char(&self) -> Result<char, Error> {
        self.char_at(self.pos)
    }

    pub fn peek_char(&self) -> Result<char, Error> {
        self.char_at(self.pos + 1)
    }

    pub fn next(&mut self) -> Result<char, Error> {
        self.pos += 1;
        self.current_char()
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.content.len()
    }

    pub fn jump(&mut self, num_chars: usize) -> Result<(), Error> {
        if self.is_eof() {
            Err(Error::Eof)
        } else {
            // If the requested number of chars to skip is larger than the remaining chars, we limit to just at EOF
            let do_num_chars = if self.pos + num_chars >= self.content.len() {
                self.content.len() - self.pos
            } else {
                num_chars
            };
            self.pos += do_num_chars;
            Ok(())
        }
    }

    pub fn is_at_line_start(&self) -> Result<bool, Error> {
        if self.pos > 0 && self.pos - 1 > self.content.len() {
            Err(Error::Eof)
        } else if self.pos == 0 {
            Ok(true)
        } else {
            let c = self.char_at(self.pos - 1).unwrap();
            match c {
                '\r' | '\n' => Ok(true),
                _ => Ok(false),
            }
        }
    }

    pub fn is_at_multiline_comment_start(&self) -> Result<bool, Error> {
        if self.is_eof() || self.pos + 1 >= self.content.len() {
            Ok(false)
        } else {
            let c = self.current_char().unwrap();
            let n = self.peek_char().unwrap();
            Ok(c == '/' && n == '*')
        }
    }

    pub fn is_at_multiline_comment_end(&self) -> Result<bool, Error> {
        if self.pos + 1 >= self.content.len() {
            Ok(false)
        } else {
            let c = self.current_char().unwrap();
            let n = self.peek_char().unwrap();
            Ok(c == '*' && n == '/')
        }
    }

    pub fn skip_multiline_comment(&mut self) -> Result<String, Error> {
        if !self.is_at_multiline_comment_start().unwrap() {
            Err(Error::CommentIsntComment)
        } else {
            let mut comment_text = "".to_string();
            while !self.is_at_multiline_comment_end().unwrap() {
                comment_text.push(self.next().unwrap());
            }
            self.jump(2).unwrap();
            Ok(comment_text[1..(comment_text.len() - 2)].to_string())
        }
    }

    pub fn is_at_pointer(&self) -> Result<bool, Error> {
        match self.current_char() {
            Ok(c) => Ok(c == '^'),
            Err(why) => Err(why),
        }
    }

    pub fn is_at_group(&self) -> Result<bool, Error> {
        if self.pos + 5 >= self.content.len() {
            Err(Error::Eof)
        } else {
            let mut s = String::new();

            s.push(self.char_at_pos_plus_n(self.pos).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 1).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 2).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 3).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 4).unwrap());

            Ok(s == "GROUP")
        }
    }

    pub fn is_at_object(&self) -> Result<bool, Error> {
        if self.pos + 5 >= self.content.len() {
            Err(Error::Eof)
        } else {
            let mut s = String::new();

            s.push(self.char_at_pos_plus_n(self.pos).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 1).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 2).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 3).unwrap());
            s.push(self.char_at_pos_plus_n(self.pos + 4).unwrap());

            Ok(s == "OBJECT")
        }
    }

    pub fn read_symbol(&mut self) -> Result<Symbol, Error> {
        if self.is_at_value_line_continuation().unwrap() {
            Err(Error::Syntax(
                "Value line continuation without a preceeding key value pair".to_owned(),
            ))
        } else if !self.is_at_line_start().unwrap() {
            Err(Error::Programming(
                "Attempt to read a key value pair when not at beginning of a line".to_owned(),
            ))
        } else {
            let mut symbol_text = String::new();
            while !self.is_eof() {
                let c = self.current_char().unwrap();
                if c != '\n' && c != '\r' && c != '=' {
                    symbol_text.push(c);
                } else {
                    break;
                }
                self.next().unwrap();
            }

            symbol_text = symbol_text.trim().to_owned();
            // println!("{} -> {}", symbol_text.len(), symbol_text);
            if symbol_text.len() == 0 {
                Ok(Symbol::BlankLine)
            } else if symbol_text.chars().nth(0).unwrap() == '^' {
                Ok(Symbol::Pointer(symbol_text))
            } else if symbol_text == "GROUP" {
                Ok(Symbol::Group)
            } else if symbol_text == "OBJECT" {
                Ok(Symbol::Object)
            } else {
                Ok(Symbol::Key(symbol_text))
            }
        }
    }

    pub fn read_remaining_line(&mut self) -> Result<String, Error> {
        let mut line_text = String::new();
        while !self.is_eof() {
            if self.current_char().unwrap() == '=' {
                self.jump(2).unwrap();
            }
            let c = self.current_char().unwrap();
            if c != '\n' && c != '\r' {
                line_text.push(c);
            } else {
                break;
            }
            if !self.is_eof() {
                self.next();
            }
        }

        line_text = line_text.trim().to_owned();
        Ok(line_text)
    }

    pub fn is_at_equals(&self) -> Result<bool, Error> {
        match self.current_char() {
            Ok(c) => Ok(c == '='),
            Err(why) => Err(why),
        }
    }

    pub fn is_at_value_line_continuation(&self) -> Result<bool, Error> {
        if !self.is_at_line_start().unwrap() {
            Ok(false)
        } else if self.pos + 37 >= self.content.len() {
            Err(Error::Eof)
        } else {
            Ok(&self.content[self.pos..(self.pos + 37)] == "                                     ")
        }
    }

    pub fn read_key_value_pair_raw(&mut self) -> Result<KeyValuePairRaw, Error> {
        if self.is_at_value_line_continuation().unwrap() {
            Err(Error::Syntax(
                "Value line continuation without a preceeding key value pair".to_owned(),
            ))
        } else if !self.is_at_line_start().unwrap() {
            Err(Error::Programming(
                "Attempt to read a key value pair when not at beginning of a line".to_owned(),
            ))
        } else {
            let mut value_string = String::new();
            let key_res = self.read_symbol();
            value_string += self.read_remaining_line().unwrap().to_string().as_ref();

            self.next();
            while let Ok(b) = self.is_at_value_line_continuation() {
                if b {
                    value_string += self.read_remaining_line().unwrap().to_string().as_ref();
                    self.next();
                } else {
                    break;
                }
            }

            Ok(KeyValuePairRaw {
                key: key_res.unwrap(),
                value: Value::new(&value_string),
            })
        }
    }
}

pub fn main() {
    let p = "/Users/kgill/data/MSL/3849/NCAM/NRB_739187404EDR_S1011354NCAM00593M1.LBL";
    let pvl_text = fs::read_to_string(p).expect("Failed to load PVL label");

    let mut reader = PvlReader::new(&pvl_text);

    while !reader.is_eof() {
        if reader.is_at_multiline_comment_start().unwrap() {
            let comment = reader.skip_multiline_comment().unwrap();
            println!("COMMENT: {}", comment);
        } else if reader.is_at_line_start().unwrap() {
            match reader.read_key_value_pair_raw() {
                Ok(kvp) => println!("KVP: {:?}", kvp),
                Err(_) => {}
            };

            // if reader.is_at_value_line_continuation().unwrap() {
            //     let continuation = reader.read_remaining_line().unwrap();
            //     println!("Continuation: {}", continuation);
            // } else {
            //     let symbol = reader.read_symbol().unwrap();
            //     println!("Symbol: {:?}", symbol);
            //     let value = reader.read_remaining_line().unwrap();
            //     println!("Value: {}", value);
            // }
        }
        reader.next();
    }
}

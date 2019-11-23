use rmpv::decode::read_value;
use rmpv::encode::write_value;
use rmpv::Value;
use std::error::Error;
use std::io;
use std::io::{Read, Write};

#[derive(Debug, PartialEq, Clone)]
pub enum RpcMessage {
    RpcRequest {
        msgid: u64,
        method: String,
        params: Vec<Value>,
    }, // 0
    RpcResponse {
        msgid: u64,
        error: Value,
        result: Value,
    }, // 1
    RpcNotification {
        method: String,
        params: Vec<Value>,
    }, // 2
}

macro_rules! try_str {
    ($exp:expr, $msg:expr) => {
        match $exp {
            Value::String(val) => match val.into_str() {
                Some(s) => s,
                None => return Err(Box::new(io::Error::new(io::ErrorKind::Other, $msg))),
            },
            _ => return Err(Box::new(io::Error::new(io::ErrorKind::Other, $msg))),
        }
    };
}

macro_rules! try_int {
    ($exp:expr, $msg:expr) => {
        match $exp.as_u64() {
            Some(val) => val,
            _ => return Err(Box::new(io::Error::new(io::ErrorKind::Other, $msg))),
        }
    };
}

macro_rules! try_arr {
    ($exp:expr, $msg:expr) => {
        match $exp {
            Value::Array(arr) => arr,
            _ => return Err(Box::new(io::Error::new(io::ErrorKind::Other, $msg))),
        }
    };
}

macro_rules! rpc_args {
    ($($e:expr), *) => {{
        let mut vec = Vec::new();
        $(
            vec.push(Value::from($e));
        )*
        Value::from(vec)
    }}
}

pub fn decode<R: Read>(reader: &mut R) -> Result<RpcMessage, Box<dyn Error>> {
    let mut arr = try_arr!(read_value(reader)?, "Rpc message must be array");
    match try_int!(arr[0], "Can't find message type") {
        0 => {
            arr.truncate(4);
            let params = try_arr!(arr.pop().unwrap(), "params not found"); // [3]
            let method = try_str!(arr.pop().unwrap(), "method not found"); // [2]
            let msgid = try_int!(arr.pop().unwrap(), "msgid not found"); // [1]

            Ok(RpcMessage::RpcRequest {
                msgid,
                method,
                params,
            })
        }
        1 => {
            arr.truncate(4);
            let msgid = try_int!(arr[1], "msgid not found");
            let result = arr.pop().unwrap(); // [3]
            let error = arr.pop().unwrap(); // [2]
            Ok(RpcMessage::RpcResponse {
                msgid,
                error,
                result,
            })
        }
        2 => {
            arr.truncate(3);
            let params = try_arr!(arr.pop().unwrap(), "params not found"); // [2]
            let method = try_str!(arr.pop().unwrap(), "method not found"); // [1]
            Ok(RpcMessage::RpcNotification { method, params })
        }
        _ => Err(Box::new(io::Error::new(
            io::ErrorKind::Other,
            "Not nown type",
        ))),
    }
}

pub fn encode<W: Write>(writer: &mut W, msg: RpcMessage) -> Result<(), Box<dyn Error>> {
    match msg {
        RpcMessage::RpcRequest {
            msgid,
            method,
            params,
        } => {
            let val = rpc_args!(0, msgid, method, params);
            write_value(writer, &val)?;
        }
        RpcMessage::RpcResponse {
            msgid,
            error,
            result,
        } => {
            let val = rpc_args!(1, msgid, error, result);
            write_value(writer, &val)?;
        }
        RpcMessage::RpcNotification { method, params } => {
            let val = rpc_args!(2, method, params);
            write_value(writer, &val)?;
        }
    };

    writer.flush()?;

    Ok(())
}

pub trait FromVal<T> {
    fn from_val(_: T) -> Self;
}

impl FromVal<Value> for () {
    fn from_val(_: Value) -> Self {
        ()
    }
}

impl FromVal<Value> for Value {
    fn from_val(val: Value) -> Self {
        val
    }
}

impl FromVal<Value> for Vec<(Value, Value)> {
    fn from_val(val: Value) -> Self {
        if let Value::Map(vec) = val {
            return vec;
        }
        panic!("Not supported value for map");
    }
}

impl<T: FromVal<Value>> FromVal<Value> for Vec<T> {
    fn from_val(val: Value) -> Self {
        if let Value::Array(arr) = val {
            return arr.into_iter().map(T::from_val).collect();
        }
        panic!("Can't convert to array");
    }
}

impl FromVal<Value> for (i64, i64) {
    fn from_val(val: Value) -> Self {
        let res = val
            .as_array()
            .expect("Can't convert to point(i64,i64) value");
        if res.len() != 2 {
            panic!("Array length must be 2");
        }

        (
            res[0].as_i64().expect("Can't get i64 value at position 0"),
            res[1].as_i64().expect("Can't get i64 value at position 1"),
        )
    }
}

impl FromVal<Value> for bool {
    fn from_val(val: Value) -> Self {
        if let Value::Boolean(res) = val {
            return res;
        }
        panic!("Can't convert to bool");
    }
}

impl FromVal<Value> for String {
    fn from_val(val: Value) -> Self {
        val.as_str().expect("Can't convert to string").to_owned()
    }
}

impl FromVal<Value> for i64 {
    fn from_val(val: Value) -> Self {
        val.as_i64().expect("Can't convert to i64")
    }
}

pub trait IntoVal<T> {
    fn into_val(self) -> T;
}

impl<'a> IntoVal<Value> for &'a str {
    fn into_val(self) -> Value {
        Value::from(self)
    }
}

impl IntoVal<Value> for Vec<String> {
    fn into_val(self) -> Value {
        let vec: Vec<Value> = self.into_iter().map(Value::from).collect();
        Value::from(vec)
    }
}

impl IntoVal<Value> for Vec<Value> {
    fn into_val(self) -> Value {
        Value::from(self)
    }
}

impl IntoVal<Value> for (i64, i64) {
    fn into_val(self) -> Value {
        Value::from(vec![Value::from(self.0), Value::from(self.1)])
    }
}

impl IntoVal<Value> for bool {
    fn into_val(self) -> Value {
        Value::from(self)
    }
}

impl IntoVal<Value> for i64 {
    fn into_val(self) -> Value {
        Value::from(self)
    }
}

impl IntoVal<Value> for String {
    fn into_val(self) -> Value {
        Value::from(self)
    }
}

impl IntoVal<Value> for Value {
    fn into_val(self) -> Value {
        self
    }
}

impl IntoVal<Value> for Vec<(Value, Value)> {
    fn into_val(self) -> Value {
        Value::from(self)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{Cursor, Seek, SeekFrom};

    #[test]
    fn request_test() {
        let msg = RpcMessage::RpcRequest {
            msgid: 1,
            method: "test_method".to_owned(),
            params: vec![],
        };

        let mut buff = Cursor::new(vec![]);
        encode(&mut buff, msg.clone()).unwrap();

        buff.seek(SeekFrom::Start(0)).unwrap();
        let msg_dest = decode(&mut buff).unwrap();
        assert_eq!(msg, msg_dest);
    }
}

#[macro_export]
macro_rules! ok_some {
    ($val: expr) => {
        match $val {
            Ok(Some(val)) => val,
            Ok(None) => return Ok(None),
            Err(err) => return Err(err.into()),
        }
    };
}

#[macro_export]
macro_rules! some_ok {
    ($val: expr) => {
        match $val {
            Some(Ok(val)) => val,
            Some(Err(err)) => return Some(Err(err.into())),
            None => return None,
        }
    };
}

#[macro_export]
macro_rules! option_ok {
    ($val: expr) => {
        match $val {
            Ok(val) => val,
            Err(err) => return Some(Err(err.into())),
        }
    };
}

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

#[macro_export]
macro_rules! some_or_return {
    ($value:expr) => {
        match $value {
            Some(value) => value,
            None => return,
        }
    };
}

#[macro_export]
macro_rules! some_or_continue {
    ($value:expr) => {
        match $value {
            Some(value) => value,
            None => continue,
        }
    };
}

#[macro_export]
macro_rules! ok_or_return {
    ($value:expr) => {
        match $value {
            Ok(value) => value,
            Err(_) => return,
        }
    };
}

#[macro_export]
macro_rules! ok_or_continue {
    ($value:expr) => {
        match $value {
            Ok(value) => value,
            Err(_) => continue,
        }
    };
}

use std::ffi::CString;

pub fn cstring_to_string(cstring: &CString, data_type: &str) -> String {
    cstring.clone().into_string().unwrap_or(format!("Error reading {data_type}!"))
}

pub fn string_to_cstring(string: String) -> CString {
    CString::new(string).unwrap_or_default()
}

pub fn repeat_all_elements<T: Clone>(ts: Vec<T>, repeated: usize) -> Vec<T> {
    let mut result = Vec::new();
    for e in ts {
        for _ in 0..repeated {
            result.push(e.clone());
        }
    }

    result
}

pub fn insert_after_every<T: Clone>(ts: Vec<T>, after: usize, elem: T) -> Vec<T> {
    let mut result = Vec::new();
    for (i, e) in ts.into_iter().enumerate() {
        result.push(e);
        if (i + 1) % after == 0 {
            result.push(elem.clone());
        }
    }

    result
}

pub fn sane_fixed_to_float(fixed: i32) -> f64 {
    if fixed == i32::MIN {
        return -32768.0;
    }
    
    let mut c = fixed.abs();
    let mut sign = 1;

    if fixed < 0 {
        c = fixed - 1;
        c = !c;
        sign = -1;
    }

    ((1.0 * f64::from(c)) / f64::from(2i32.pow(16))) * f64::from(sign)
}

pub fn float_to_sane_fixed(float: f64) -> i32 {
    if float <= -32768.0 {
        return i32::MIN;
    }

    let a = float * f64::from(2i32.pow(16));
    #[allow(clippy::cast_possible_truncation)]
    let mut b = a.round() as i32;

    if a < 0.0 {
        b = b.abs();
        b = !b;
        b += 1;
    }

    b
}
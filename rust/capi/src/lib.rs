use hyperon::*;
use std::ffi::*;
use std::os::raw::*;
use std::convert::TryInto;
use std::fmt::Display;

#[allow(non_camel_case_types)]
pub struct atom_t {
    atom: Atom,
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct gnd_api_t {
    // One can assign NULL to this field, it means the atom is not executable
    execute: Option<extern "C" fn(*const gnd_t, *mut vec_atom_t, *mut vec_atom_t) -> *const c_char>,
    eq: extern "C" fn(*const gnd_t, *const gnd_t) -> bool,
    clone: extern "C" fn(*const gnd_t) -> *mut gnd_t,
    display: extern "C" fn(*const gnd_t, *mut c_char, usize) -> usize,
    free: extern "C" fn(*mut gnd_t),
}

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct gnd_t {
    api: *const gnd_api_t,
}

#[no_mangle]
pub unsafe extern "C" fn atom_sym(name: *const c_char) -> *mut atom_t {
    // cstr_as_str() keeps pointer ownership, but Atom::sym() copies resulting
    // String into Atom::Symbol::symbol field. atom_to_ptr() moves value to the
    // heap and gives ownership to the caller.
    atom_to_ptr(Atom::Symbol(cstr_as_str(name).into()))
}

// TODO: Think about changing the API to make resulting expression taking ownership of passed
// values
#[no_mangle]
pub unsafe extern "C" fn atom_expr(children: *const *mut atom_t, size: usize) -> *mut atom_t {
    let children: Vec<Atom> = std::slice::from_raw_parts(children, size).iter().map(|p| (**p).atom.clone()).collect();
    atom_to_ptr(Atom::Expression(children.into()))
}

#[no_mangle]
pub unsafe extern "C" fn atom_var(name: *const c_char) -> *mut atom_t {
    atom_to_ptr(Atom::Variable(cstr_as_str(name).into()))
}

#[no_mangle]
pub extern "C" fn atom_gnd(gnd: *mut gnd_t) -> *mut atom_t {
    atom_to_ptr(Atom::gnd(CGroundedAtom(gnd)))
}

#[no_mangle]
pub unsafe extern "C" fn free_atom(atom: *mut atom_t) {
    // drop() does nothing actually, but it is used here for clarity
    drop(Box::from_raw(atom));
}

#[no_mangle]
pub unsafe extern "C" fn atom_to_str(atom: *const atom_t, buffer: *mut c_char, max_size: usize) -> usize {
    string_to_cstr(format!("{}", (*atom).atom), buffer, max_size)
}

#[allow(non_camel_case_types)]
pub struct vec_atom_t<'a>(&'a mut Vec<Atom>);

#[no_mangle]
pub unsafe extern "C" fn vec_pop(vec: *mut vec_atom_t) -> *const atom_t {
    atom_to_ptr((*vec).0.pop().expect("Vector is empty")) as *const atom_t
}

////////////////////////////////////////////////////////////////
// Code below is a boilerplate code to implement C API correctly

fn atom_to_ptr(atom: Atom) -> *mut atom_t {
    Box::into_raw(Box::new(atom_t{ atom }))
}

// C grounded atom wrapper

struct CGroundedAtom(*mut gnd_t);

impl CGroundedAtom {

    fn as_ptr(&self) -> *mut gnd_t {
        self.0
    }

    fn api(&self) -> &gnd_api_t {
        unsafe {
            &*(*self.as_ptr()).api
        }
    }

    unsafe fn execute(&self, ops: &mut Vec<Atom>, data: &mut Vec<Atom>) -> Result<(), String> {
        let execute = self.api().execute;
        match execute {
            Some(execute) => {
                let res = execute(self.as_ptr(), &mut vec_atom_t(ops), &mut vec_atom_t(data));
                if res.is_null() {
                    Err(cstr_as_str(res).to_string())
                } else {
                    Ok(())
                }
            },
            None => Err(format!("{} is not executable", self)),
        }
    }

    fn eq(&self, other: &Self) -> bool {
        (self.api().eq)(self.as_ptr(), other.as_ptr())
    }

    fn clone(&self) -> Self {
        CGroundedAtom((self.api().clone)(self.as_ptr()))
    }

    unsafe fn display(&self) -> &str {
        let mut buffer = [0; 4096];
        (self.api().display)(self.as_ptr(), buffer.as_mut_ptr().cast::<c_char>(), 4096);
        cstr_as_str(buffer.as_ptr().cast::<c_char>())
    }

    fn free(&self) {
        (self.api().free)(self.as_ptr());
    }

}

impl GroundedAtom for CGroundedAtom {

    fn execute(&self, ops: &mut Vec<Atom>, data: &mut Vec<Atom>) -> Result<(), String> {
        unsafe {
            self.execute(ops, data)
        }
    }

    fn eq_gnd(&self, other: &dyn GroundedAtom) -> bool {
        match other.downcast_ref::<CGroundedAtom>() {
            Some(o) => self.eq(o),
            None => false,
        }
    }

    fn clone_gnd(&self) -> Box<dyn GroundedAtom> {
        Box::new(self.clone())
    }

}

impl Display for CGroundedAtom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            write!(f, "{}", self.display())
        }
    }
}

impl Drop for CGroundedAtom {
    fn drop(&mut self) {
        self.free();
    }
}

// String conversion utilities

unsafe fn string_to_cstr(s: String, buffer: *mut c_char, max_size: usize) -> usize {
    let bytes = s.as_bytes();
    if !buffer.is_null() {
        let len = std::cmp::min(bytes.len(), max_size - 4);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), len);
        //let buffer = std::slice::from_raw_parts_mut::<c_char>(buffer, max_size);
        let len : isize = len.try_into().unwrap();
        if bytes.len() > max_size - 4 {
            buffer.offset(len).write('.' as c_char);
            buffer.offset(len + 1).write('.' as c_char);
            buffer.offset(len + 2).write('.' as c_char);
            buffer.offset(len + 3).write(0);
        } else {
            buffer.offset(len).write(0);
        }
    }
    bytes.len()
}

unsafe fn cstr_as_str<'a>(s: *const c_char) -> &'a str {
    CStr::from_ptr(s).to_str().expect("Incorrect UTF-8 sequence")
}

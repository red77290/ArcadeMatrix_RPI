#[derive(Debug)]
struct CMatrix;

struct LedMatrix {
    handle: *mut CMatrix,
    options: u32,
}

fn main() {
    let m = LedMatrix {
        handle: 0x12345678 as *mut CMatrix,
        options: 42,
    };
    
    let ptr = &m as *const _ as *const *mut std::ffi::c_void;
    let handle = unsafe { *ptr };
    println!("Handle: {:?}", handle);
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> i32 {
    // Call gas metering function to simulate gas usage
    unsafe {
        gas(10);  // Use 10 gas units for this operation
    }
    a + b
}

#[wasm_bindgen]
pub fn loop_test(iterations: i32) {
    for _ in 0..iterations {
        // Call gas metering function each iteration
        unsafe {
            gas(1);  // Use 1 gas unit per iteration
        }
    }
}

// Import gas metering function from host environment
#[link(wasm_import_module = "env")]
extern "C" {
    fn gas(amount: i32);
}

// Export memory to host environment
#[no_mangle]
static mut MEMORY: [u8; 65536] = [0; 65536];

// Memory management functions
#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    let ptr = unsafe { MEMORY.as_mut_ptr() };
    ptr
}

#[no_mangle]
pub extern "C" fn dealloc(_ptr: *mut u8, _size: usize) {
    // Memory is static, no need to deallocate
}

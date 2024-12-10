use wasm_bindgen::prelude::*;

// State storage
static mut STATE: i32 = 0;
static VERSION: i32 = 2;

#[wasm_bindgen]
pub fn store_value(value: i32) {
    unsafe {
        gas(10);  // Use 10 gas units for state modification
        STATE = value;
    }
}

#[wasm_bindgen]
pub fn get_value() -> i32 {
    unsafe {
        gas(5);  // Use 5 gas units for state read
        STATE
    }
}

#[wasm_bindgen]
pub fn get_version() -> i32 {
    unsafe {
        gas(1);  // Use 1 gas unit for version check
    }
    VERSION
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

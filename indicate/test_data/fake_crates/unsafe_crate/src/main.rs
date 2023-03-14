fn main() {
    let m = std::mem::size_of::<i64>();

    // Call to unsafe function
    let p: *const i64 = unsafe { libc::malloc(m) as *const i64 };

    // `p` is never checked against `libc::PT_NULL` to ensure
    // memory allocation succeeded

    // Unsafe dereference of raw pointer
    println!("Found {} at allocated memory", unsafe { *p });

    // `p` is never freed!
}


use std::{thread, time};
use std::alloc::{GlobalAlloc, Layout};
use std::alloc::System;

use std::sync::atomic::{AtomicI32, Ordering};

static mut ALLOC_COUNT : AtomicI32 = AtomicI32::new(0);
static mut DEALLOC_COUNT : AtomicI32 = AtomicI32::new(0);

struct MyAllocator;
unsafe impl GlobalAlloc for MyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        System.alloc(_layout) 
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        DEALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        System.dealloc(_ptr, _layout)
    }
}

pub struct MemScope {
    alloc : i32,
    dealloc : i32, 
}
impl MemScope {
    pub fn new() -> MemScope {
        unsafe {
            MemScope {
                alloc : ALLOC_COUNT.load(Ordering::SeqCst),
                dealloc : DEALLOC_COUNT.load(Ordering::SeqCst),
            }
        }
    }
}
impl Drop for MemScope {
    fn drop(&mut self) {
        unsafe {
            let alloc = ALLOC_COUNT.load(Ordering::SeqCst);
            let dealloc = DEALLOC_COUNT.load(Ordering::SeqCst);
            println!("New Allocs {}\nWorking {}\n", alloc - self.alloc, dealloc - self.dealloc);
        }

    }
}


pub struct TestScope {
    string : &'static str
}
impl TestScope {
    pub fn new(string : &'static str) -> TestScope {
        println!("Scope Entry {}", string);
        TestScope {
            string
        }
    }
}
impl Drop for TestScope {
    fn drop(&mut self) {
        println!("Scope Exit {}", self.string);
    }
}

use_profile_memory_allocator!();
//#[global_allocator]
//static A: MyAllocator = MyAllocator;

extern crate rs_tracing;
extern crate backtrace;

use rs_tracing::*;
mod profiler;
use backtrace::*;

fn main() {

    {
        let _a = TestScope::new("a");
        let _b = TestScope::new("b");
        let _c = TestScope::new("c");                
    }


    profile_start!(100_000);

    let thread1 = thread::Builder::new().name("child1".to_string()).spawn(move || {
        profile_scope!("Test scope");
        profile_scope!("Test scope2");
        {
          profile_scope!("Test scope3");
          let bt = Backtrace::new();
          println!("Stack {:?}", bt);
        }
        profile_begin!("Test area");
        thread::sleep(time::Duration::from_millis(5));

        let test_alloc = vec!(0;100);

        thread::sleep(time::Duration::from_millis(5));
        println!("{}", test_alloc[5]);
        profile_end!();
    }).unwrap();

    let thread2 = thread::Builder::new().name("child2".to_string()).spawn(move || {
        profile_scope!("Test scope");
        profile_scope!("Test scope2");
        {
          profile_scope!("Test scope3");
        }
        profile_begin!("Test area");
        thread::sleep(time::Duration::from_millis(5));

        let test_alloc = vec!(0;100);

        thread::sleep(time::Duration::from_millis(5));
        println!("{}", test_alloc[5]);
        profile_end!();
    }).unwrap();

    {
        //let _mem_test = MemScope::new();
        profile_scope!("Test scope");
        profile_scope!("Test scope2");
        {
          profile_scope!("Test scope3");
        }
        profile_begin!("Test area");
        thread::sleep(time::Duration::from_millis(5));

        let test_alloc = vec!(0;100);

        thread::sleep(time::Duration::from_millis(5));
        println!("{}", test_alloc[5]);
        profile_end!();
    }
    thread1.join().unwrap();
    thread2.join().unwrap();

    let _ = profile_finish_to_file!("test2.txt");

    open_trace_file!("foo.txt").unwrap();
    {
        //let _mem_test = MemScope::new();        
        trace_scoped!("complete");
        trace_scoped!("complete2");
        trace_scoped!("complete3");
        //trace_expr!("trace_expr", println!("trace_expr"));
        trace_begin!("duration");
        thread::sleep(time::Duration::from_millis(10));
        trace_end!("duration");
    }
    close_trace_file!();
}


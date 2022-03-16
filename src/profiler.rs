
#[macro_export]
macro_rules! profile_start {
    ($tag_count: expr) => {
        $crate::profiler::internal::begin($tag_count)
    };
}

#[macro_export]
macro_rules! profile_finish {
    ($writer: expr) => {
        $crate::profiler::internal::end($writer)
    };
}

#[macro_export]
macro_rules! profile_finish_to_file {
    ($filename: expr) => {
        $crate::profiler::internal::end_to_file($filename)
    };
}

#[macro_export]
macro_rules! profile_begin {
    ($tag: expr) => {
        $crate::profiler::internal::profile_begin($tag)
    };
}

#[macro_export]
macro_rules! profile_end {
    () => {
        $crate::profiler::internal::profile_end()
    };
}

#[macro_export]
macro_rules! profile_scope {
    ($tag: expr) => {
        let _profile_guard = $crate::profiler::internal::ProfileScope::new($tag);
    };
}

#[macro_export]
macro_rules! use_profile_memory_allocator {
    () => {
        #[global_allocator]
        static A: $crate::profiler::internal::MemTrackAllocator = $crate::profiler::internal::MemTrackAllocator;
    };
}

#[cfg(windows)]
mod sys {
    use std::mem;
    use std::cell::UnsafeCell;    
    use winapi::um::winnt::*;
    use winapi::um::profileapi::*;
    use winapi::um::synchapi::*;
    use winapi::um::minwinbase::*;
    use winapi::um::dbghelp::*;
    use winapi::um::processthreadsapi::*;
    use std::ops::{Deref, DerefMut};
    use winapi::shared::minwindef::{
        TRUE, BOOL, DWORD, HMODULE, LPDWORD, PDWORD, PUCHAR, PULONG, UCHAR, ULONG, USHORT, WORD,
    };

    #[derive(PartialEq, Clone, Copy)]
    pub struct TimePoint(i64);
    pub struct StopWatch {
        frequency : i64
    }

    impl StopWatch {
        pub fn new() -> StopWatch {
            let frequency;
            unsafe {
                let mut l : LARGE_INTEGER = mem::zeroed();
                QueryPerformanceFrequency(&mut l);
                frequency = *l.QuadPart();
            }

            StopWatch { 
                frequency
            }
        }        

        pub fn get_time() -> TimePoint {
            unsafe {
                let mut l : LARGE_INTEGER = mem::zeroed();                
                QueryPerformanceCounter(&mut l);
                TimePoint(*l.QuadPart())
            }
        }

        pub fn get_milliseconds(&self, a : &TimePoint, b : &TimePoint) -> i64 {
            mul_div_i64(b.0 - a.0, 1000_000, self.frequency)
        }
    }

    pub fn get_thread_id() -> u32 {
        unsafe { GetCurrentThreadId() }
    }

    pub struct ReentrantMutex<T: ?Sized> {
         inner : Box<CRITICAL_SECTION>,
         lock_count : u32,
         data: UnsafeCell<T>
    }
    pub struct MutexGuard<'a, T: ?Sized + 'a> {
        // funny underscores due to how Deref/DerefMut currently work (they
        // disregard field privacy).
        __lock: &'a mut ReentrantMutex<T>
    }

    impl<T> ReentrantMutex<T> {
        pub fn new(t: T) -> ReentrantMutex<T> {
            unsafe {
                let mut ret = ReentrantMutex {
                     inner: Box::new(mem::zeroed()),
                     lock_count : 0, 
                     data: UnsafeCell::new(t) 
                };
                InitializeCriticalSection(&mut *ret.inner);
                ret
            }
        }
    }

    impl<T: ?Sized> Drop for ReentrantMutex<T> {
        fn drop(&mut self) {
            unsafe {
                DeleteCriticalSection(&mut *self.inner);
            }
        }
    }

    impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
        pub fn new(lock: &'mutex mut ReentrantMutex<T>) -> Result<MutexGuard<'mutex, T>,()> {
            unsafe {
                EnterCriticalSection(&mut *lock.inner);
                lock.lock_count += 1;
                Ok(MutexGuard { __lock: lock })
            }
        }
    }

    impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
        pub fn new_no_recurse(lock: &'mutex mut ReentrantMutex<T>) -> Result<MutexGuard<'mutex, T>,()> {
            unsafe {
                EnterCriticalSection(&mut *lock.inner);
                if lock.lock_count > 0 {
                    LeaveCriticalSection(&mut *lock.inner);
                    return Err(());
                }

                lock.lock_count += 1;
                Ok(MutexGuard { __lock: lock })
            }
        }
    }

    impl<T: ?Sized> Drop for MutexGuard<'_, T> {
        #[inline]
        fn drop(&mut self) {
            unsafe {
                self.__lock.lock_count -= 1;
                LeaveCriticalSection(&mut *self.__lock.inner);
            }
        }
    }

    impl<T: ?Sized> Deref for MutexGuard<'_, T> {
        type Target = T;

        fn deref(&self) -> &T {
            unsafe { &*self.__lock.data.get() }
        }
    }

    impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut *self.__lock.data.get() }
        }
    }

    // Computes (value*numer)/denom without overflow, as long as both
    // (numer*denom) and the overall result fit into i64 (which is the case
    // for our time conversions).
    pub fn mul_div_i64(value: i64, numer: i64, denom: i64) -> i64 {
        let q = value / denom;
        let r = value % denom;
        // Decompose value as (value/denom*denom + value%denom),
        // substitute into (value*numer)/denom and simplify.
        // r < denom, so (denom*numer) is the upper bound of (r*numer)
        q * numer + r * numer / denom
    }
/*
    pub unsafe fn trace(cb: &mut FnMut(&super::Frame) -> bool) {
        // Allocate necessary structures for doing the stack walk
        let process = GetCurrentProcess();
        let thread = GetCurrentThread();

        let mut context = mem::zeroed::<MyContext>();
        RtlCaptureContext(&mut context.0);

        // Attempt to use `StackWalkEx` if we can, but fall back to `StackWalk64`
        // since it's in theory supported on more systems.
        let mut frame = super::Frame {
            inner: Frame::New(mem::zeroed()),
        };
        let image = init_frame(&mut frame.inner, &context.0);
        let frame_ptr = match &mut frame.inner {
            Frame::New(ptr) => ptr as *mut STACKFRAME_EX,
            _ => unreachable!(),
        };

        while StackWalkEx(
            image as DWORD,
            process,
            thread,
            frame_ptr,
            &mut context.0 as *mut CONTEXT as *mut _,
            None,
            Some(SymFunctionTableAccess64()),
            Some(SymGetModuleBase64()),
            None,
            0,
        ) == TRUE
        {
            if !cb(&frame) {
                break;
            }
        }
    }
*/
}

pub mod internal {
    use super::sys;
    use backtrace::*;

    use std::io;
    use std::io::{Write, BufWriter};
    use std::alloc::{System, GlobalAlloc, Layout};

    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::collections::HashMap;
    use std::ops::DerefMut;

    enum TagType
    {
        Begin(&'static str),
        End,
        Complete(&'static str, i64), // A complete event holds a duration of the event
        Allocate(usize),
        Deallocate(usize)
    }
 
    struct ProfileRecord {
        time : sys::TimePoint,        // The time of the profile data
        thread_id : u32,  // The id of the thread
        tag : TagType,         // The tag used in profiling - if empty is an end event
    }

    struct Tags {
        index : usize,           // The index of the thread
        tags : Vec<&'static str> // The tag stack
    }

    pub struct ProfileData {
        stopwatch : sys::StopWatch,
        start_time : sys::TimePoint,         // The start time of the profile
        enabled : bool,               // If profiling is enabled
        records : Vec<ProfileRecord>, // The profiling records
    }
    impl ProfileData {
        pub fn new() -> ProfileData {
            ProfileData {
                stopwatch : sys::StopWatch::new(), 
                start_time : sys::StopWatch::get_time(),  
                enabled : false,
                records : vec![]
            }
        }

        fn add_record(&mut self, record : ProfileRecord) -> Option<usize> {
            if !self.enabled || 
               self.records.len() >= self.records.capacity()
            {
                return None;
            }
            self.records.push(record);
            return Some(self.records.len() - 1);
        }            
    }

    pub struct ProfileScope {
        index : Option<usize>,
        time : sys::TimePoint
    }

    impl ProfileScope {
        pub fn new(name: &'static str) -> ProfileScope {
            let thread_id = sys::get_thread_id();            

            // Start as a begin tag
            let mut ret = ProfileScope { index : None, time : sys::StopWatch::get_time() };            
            if let Ok(ref mut profile) = get_profile() {
                ret.index = profile.add_record(ProfileRecord { time : ret.time, thread_id, tag : TagType::Begin(name) });
            }            
            ret
        }
    }

    impl Drop for ProfileScope {
        fn drop(&mut self) {
            if let Some(index) = self.index {
                if let Ok(ref mut profile) = get_profile() {
                    let profile = profile.deref_mut();                    
                    if index < profile.records.len() {
                        let record = &mut profile.records[index];
                        if let TagType::Begin(name) = record.tag {
                            if self.time == record.time {
                                // If the time is different, it must have started in a different profile session
                                // Change the tag type to complete
                                let duration = profile.stopwatch.get_milliseconds(&record.time, &sys::StopWatch::get_time());
                                record.tag = TagType::Complete(name, duration);
                            }
                        }
                    }
                }
            }            
        }
    }

    pub struct MemTrackAllocator;
    static TRACK_ALLOCS : AtomicBool = AtomicBool::new(false);
    impl MemTrackAllocator
    {
        pub fn set_mem_tracking(new_val : bool) {
            TRACK_ALLOCS.store(new_val, Ordering::SeqCst);
        }
        pub fn get_mem_tracking() -> bool {
            TRACK_ALLOCS.load(Ordering::SeqCst)
        }
    }

    unsafe impl GlobalAlloc for MemTrackAllocator {
        unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
            if MemTrackAllocator::get_mem_tracking() {
                if let Ok(ref mut profile) = get_profile_no_recurse() {                
                    //let bt = Backtrace::new_unresolved();
                    //println!("Stack {:?}", bt);
                    backtrace::trace_unsynchronized(|frame| { true });
                    //backtrace::trace(|frame| { true });

                    let time = sys::StopWatch::get_time();
                    let thread_id = sys::get_thread_id();
                    profile.add_record(ProfileRecord { thread_id, tag : TagType::Allocate(_layout.size()), time });
                }            
            }
            System.alloc(_layout) 
        }

        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
            if MemTrackAllocator::get_mem_tracking() {
                if let Ok(ref mut profile) = get_profile_no_recurse() {                                
                    let time = sys::StopWatch::get_time();
                    let thread_id = sys::get_thread_id();
                    profile.add_record(ProfileRecord { thread_id, tag : TagType::Deallocate(_layout.size()), time });
                }             
            }
            System.dealloc(_ptr, _layout)
        }
    }

    pub fn profile_begin(tag : &'static str)
    {
        let thread_id = sys::get_thread_id();
        if let Ok(ref mut profile) = get_profile() {
            profile.add_record(ProfileRecord { thread_id, tag : TagType::Begin(tag), time : sys::StopWatch::get_time() });
        }            
    }

    pub fn profile_end()
    {
        let time = sys::StopWatch::get_time(); // Always get time as soon as possible
        let thread_id = sys::get_thread_id();
        if let Ok(ref mut profile) = get_profile() {
            profile.add_record(ProfileRecord { thread_id, tag : TagType::End, time });
        }            
    }

    pub fn begin(record_count : usize) {
        if let Ok(ref mut profile) = get_profile() {
            // Abort if already enabled
            if profile.enabled {
                return;
            }

            profile.records.clear();
            profile.records.reserve(record_count);
            profile.start_time = sys::StopWatch::get_time();

            profile.enabled = true;
            MemTrackAllocator::set_mem_tracking(true);
        }            
    }

    fn clean_json_str<'a>(io_str : &'a str, str_buffer : &'a mut String) -> &'a str {
        // Check if there are any characters to replace
        if io_str.find(|c: char| (c == '\\') || (c == '"')) == None {
            return io_str;
        }

        // Escape json protected characters (not fast, but should be rare)
        *str_buffer = io_str.replace('\\', "\\\\").replace('"', "\\\"");
        return str_buffer;
    }

    pub fn end_to_file(filename : &str) -> io::Result<()> {
        MemTrackAllocator::set_mem_tracking(false);
        end(&mut BufWriter::new(std::fs::File::create(filename)?))
    }

    pub fn end(w : &mut dyn Write) -> io::Result<()> {
        MemTrackAllocator::set_mem_tracking(false);
        if let Ok(ref mut profile) = get_profile() {
            // Abort if already enabled
            if !profile.enabled {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }

            profile.enabled = false;

            // DT_TODO: Parhaps use thread::current().name() ?
            let mut thread_stack = HashMap::new();
            thread_stack.insert(sys::get_thread_id(), Tags { index : 0, tags : vec!()});

            let mut first : bool = true;
            let mut clean_buffer : String = String::new();
            let mut extra_buffer : String = String::new();

            w.write(b"{\"traceEvents\":[\n")?;
            for entry in profile.records.iter()
            {
                // Assign a unique index to each thread                
                let new_id = thread_stack.len();
                let stack = thread_stack.entry(entry.thread_id).or_insert(Tags { index : new_id, tags : vec!()});

                let tag;
                let type_tag;
                extra_buffer.clear();
                match entry.tag {
                    TagType::Begin(s) => {
                        type_tag = "B"; 
                        tag = s; 
                        stack.tags.push(tag)
                    },
                    TagType::End => {
                        type_tag = "E"; 
                        if let Some(stack_tag) = stack.tags.pop() {
                            tag = stack_tag;
                        }
                        else {
                            tag = "Unknown";
                        }
                    }
                    TagType::Complete(t, d) => {
                        type_tag = "X"; 
                        tag = t;
                        extra_buffer = format!(",\"dur\":{}", d);
                    }
                    TagType::Allocate(a) => {
                        type_tag = "O"; 
                        tag = "Allocate";
                        extra_buffer = format!(",\"id\":0,\"args\":{{\"snapshot\":{{\"amount\":{}}}}}", a);
                    }                                        
                    TagType::Deallocate(a) => {
                        type_tag = "O"; 
                        tag = "Deallocate";
                        extra_buffer = format!(",\"id\":1,\"args\":{{\"snapshot\":{{\"amount\":{}}}}}", a);                        
                    }                                        
                }

                if !first
                {
                    w.write(b",\n")?;
                }
                first = false;

                // Ensure escaped json is written
                let tag = clean_json_str(tag, &mut clean_buffer);

                // Get the microsecond count
                let tag_time = profile.stopwatch.get_milliseconds(&profile.start_time, &entry.time);

                // Format the string
                write!(w, "{{\"name\":\"{}\",\"ph\":\"{}\",\"ts\":{},\"tid\":0,\"pid\":{}{}}}",
                    tag, type_tag, tag_time, stack.index, extra_buffer)?;
            }
            w.write(b"\n]\n}\n")?;
            return Ok(());
        }
        return Err(io::Error::from(io::ErrorKind::InvalidData));
/*

// Write thread "names"
if (!first)
{
for (auto& t : threadStack)
{
    char indexString[64];
    snprintf(indexString, sizeof(indexString), "%d", t.second.m_index);

    // Sort thread listing by the time that they appear in the profile (tool sorts by name)
    char indexSpaceString[64];
    snprintf(indexSpaceString, sizeof(indexSpaceString), "%02d", t.second.m_index);

    // Ensure a clean json string
    std::stringstream ss;
    ss << t.first;
    std::string threadName = ss.str();
    CleanJsonStr(threadName);

    o_outStream <<
    ",\n{\"name\":\"thread_name\",\"ph\":\"M\",\"pid\":0,\"tid\":" << indexString <<
    ",\"args\":{\"name\":\"Thread" << indexSpaceString << "_" << threadName << "\"}}";
}
}
*/

    }          

    fn get_profile_mutex() -> &'static mut sys::ReentrantMutex<ProfileData> {
        static INIT : Once = Once::new();
        static mut GPROFILE : Option<sys::ReentrantMutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(sys::ReentrantMutex::new(ProfileData::new()));
            });
            GPROFILE.as_mut().unwrap_or_else(|| {std::hint::unreachable_unchecked()})
        }  
    }

    fn get_profile() -> Result<sys::MutexGuard<'static, ProfileData>, ()> {
        sys::MutexGuard::new(get_profile_mutex())
    }

    fn get_profile_no_recurse() -> Result<sys::MutexGuard<'static, ProfileData>, ()> {
        sys::MutexGuard::new_no_recurse(get_profile_mutex())
    }

}
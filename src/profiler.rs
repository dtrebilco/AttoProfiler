
mod internal {

    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;

    struct ProfileRecord {
        time : u64,         // The time of the profile data
        thread_id : u64,     // The id of the thread
        tag : &'static str, // The tag used in profiling - if empty is an end event
    }

    struct Tags {
        index : i32,             // The index of the thread
        tags : Vec<&'static str> // The tag stack
    }

    struct ProfileData {
        start_time : u64,        // The start time of the profile
        enabled : AtomicBool,   // If profiling is enabled
        //access : Mutex,         // Access mutex for changing data
        records : Vec<ProfileRecord>, // The profiling records
    }
    
    static mut GPROFILE : Option<Mutex<ProfileData>> = None;
}
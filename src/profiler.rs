
mod internal {

    use std::sync::Mutex;
    use std::sync::Once;

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
        enabled : bool,   // If profiling is enabled
        records : Vec<ProfileRecord>, // The profiling records
    }
    impl ProfileData {
        pub fn new() -> ProfileData {
            ProfileData { 
                start_time : 0,
                enabled : false,
                records : vec![]
            }
        }    
    }

    static INIT: Once = Once::new();
    static mut GPROFILE : Option<Mutex<ProfileData>> = None;

    fn run_on_profile(f : fn(&mut ProfileData)) {
        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            if let Some(ref mut mutex) = GPROFILE {
                if let Ok(ref mut profile) = mutex.try_lock() {
                    f(profile);
                }
            }
        }  
    }

    impl ProfileData {
        pub fn profile_begin(&mut self, tag : &'static str) // DT_TODO: pass in thread id
        {
            if !self.enabled || 
               self.records.len() >= self.records.capacity()
            {
                return;
            }

            // Create the profile record
            self.records.push(ProfileRecord { thread_id : 0, tag : tag, time : 0 });

            // Assign the time as the last possible thing
            //self.records.last_mut().time = clock::now();
        }

        pub fn profile_end(&mut self)
        {
            if !self.enabled || 
               self.records.len() >= self.records.capacity(){
                return;
            }

            //newData.m_time = clock::now(); // Always get time as soon as possible
            self.records.push(ProfileRecord { thread_id : 0, tag : "tag", time : 0 });
        }

        pub fn begin(&mut self, record_count : usize) {
            // Abort if already enabled
            if self.enabled {
                return;
            }

            self.records.clear();
            self.records.reserve(record_count);
            self.start_time = 1; // DT_TODO:

            self.enabled = true;
        }

        pub fn end(&mut self) {
            // Abort if already enabled
            if !self.enabled {
                return;
            }
            self.enabled = false;


        }          

    }



}
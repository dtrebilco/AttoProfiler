
mod internal {

    use std::sync::Mutex;
    use std::sync::Once;
    use std::io;
    use std::io::Write;
    use std::thread::{self, ThreadId};
    use std::collections::HashMap;

    enum TagType
    {
        Begin(&'static str),
        End
    }

    struct ProfileRecord {
        time : u64,            // The time of the profile data
        thread_id : ThreadId,  // The id of the thread
        tag : TagType,         // The tag used in profiling - if empty is an end event
    }

    struct Tags {
        index : usize,             // The index of the thread
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

    fn run_on_profile(f : fn(&mut ProfileData)) {
        static INIT: Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

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

    fn get_profile() ->  &'static Option<Mutex<ProfileData>> {
        static INIT: Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            &GPROFILE
        }  
    }

    fn get_profile2() ->  &'static Mutex<ProfileData> {
        static INIT: Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            &GPROFILE.as_ref().unwrap()
        }  
    }

    fn get_profile3() -> std::sync::TryLockResult<std::sync::MutexGuard<'static, ProfileData>> {
        static INIT: Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            GPROFILE.as_ref().unwrap().try_lock()
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
            self.records.push(ProfileRecord { thread_id : thread::current().id(), tag : TagType::Begin(tag), time : 0 });

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
            self.records.push(ProfileRecord { thread_id : thread::current().id(), tag : TagType::End, time : 0 });
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

        fn clean_json_str<'a>(io_str : &'a str, str_buffer : &'a mut String) -> &'a str {
            // Check if there are any characters to replace
            if io_str.find(|c: char| (c == '\\') || (c == '"')) == None {
                return io_str;
            }

            // Escape json protected characters (not fast, but should be rare)
            *str_buffer = io_str.replace('\\', "\\\\").replace('"', "\\\"");
            return str_buffer;
        }

        pub fn end(&mut self, w : &mut dyn Write) -> io::Result<()> {
            // Abort if already enabled
            if !self.enabled {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }

            self.enabled = false;

            // DT_TODO: Parhaps use thread::current().name() ?
            let mut thread_stack = HashMap::new();
            thread_stack.insert(thread::current().id(), Tags { index : 0, tags : vec!()});

            let mut first : bool = true;
            let mut clean_buffer : String = String::new();
            w.write(b"{\"traceEvents\":[\n")?;

            for entry in self.records.iter()
            {
                // Assign a unique index to each thread                
                let new_id = thread_stack.len();
                let stack = thread_stack.entry(entry.thread_id).or_insert(Tags { index : new_id, tags : vec!()});

                let tag;
                let type_tag;
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
                }

                if !first
                {
                    w.write(b",\n")?;
                }
                first = false;

                // Ensure escaped json is written
                let tag = ProfileData::clean_json_str(tag, &mut clean_buffer);

                // Get the microsecond count
                //long long msCount = std::chrono::duration_cast<std::chrono::microseconds>(entry.m_time - g_pData->m_startTime).count();

                // Format the string
                write!(w, "{{\"name\":\"{}\",\"ph\":\"{}\",\"ts\": {},\"tid\":{},\"cat\":\"\",\"pid\":0,\"args\":{{}}}}",
                    tag, type_tag, 0, stack.index)?;
            }
            w.write(b"\n]\n}\n")?;
            return Ok(());
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
    }

}
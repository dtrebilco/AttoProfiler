
use std::io::BufWriter;
use std::io::Write;

pub fn begin(tag_count : usize ) {
    if let Ok(ref mut profile) = internal::get_profile() {
        profile.begin(tag_count);
    }
}

pub fn profile_begin(tag : &'static str){
    if let Ok(ref mut profile) = internal::get_profile() {
        profile.profile_begin(tag);
    }
}

pub fn profile_end(){
    if let Ok(ref mut profile) = internal::get_profile() {
        profile.profile_end();
    }
}

pub fn end(writer : &mut dyn Write) -> std::io::Result<()> {
    if let Ok(ref mut profile) = internal::get_profile() {
        profile.end(writer)?;
    }
    Ok(())
}

pub fn end_to_file(filename : &str) -> std::io::Result<()> {
    end(&mut BufWriter::new(std::fs::File::create(filename)?))
}


mod internal {

    use std::sync::Mutex;
    use std::sync::Once;
    use std::io::Write;
    use std::io;
    
    use std::time::{Duration, Instant};
    use std::thread::{self, ThreadId};
    use std::collections::HashMap;

    enum TagType
    {
        Begin(&'static str),
        End
    }

    struct ProfileRecord {
        time : Instant,        // The time of the profile data
        thread_id : ThreadId,  // The id of the thread
        tag : TagType,         // The tag used in profiling - if empty is an end event
    }

    struct Tags {
        index : usize,           // The index of the thread
        tags : Vec<&'static str> // The tag stack
    }

    pub struct ProfileData {
        start_time : Instant,         // The start time of the profile
        enabled : bool,               // If profiling is enabled
        records : Vec<ProfileRecord>, // The profiling records
    }
    impl ProfileData {
        pub fn new() -> ProfileData {
            ProfileData { 
                start_time : Instant::now(),  
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

    pub fn get_profile() -> std::sync::TryLockResult<std::sync::MutexGuard<'static, ProfileData>> {
        static INIT : Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            GPROFILE.as_ref().unwrap_or_else(|| {std::hint::unreachable_unchecked()}).try_lock()
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
            self.records.push(ProfileRecord { thread_id : thread::current().id(), tag : TagType::Begin(tag), time : Instant::now() });
        }

        pub fn profile_end(&mut self)
        {
            if !self.enabled || 
               self.records.len() >= self.records.capacity(){
                return;
            }

            let time = Instant::now(); // Always get time as soon as possible
            self.records.push(ProfileRecord { thread_id : thread::current().id(), tag : TagType::End, time });
        }

        pub fn begin(&mut self, record_count : usize) {
            // Abort if already enabled
            if self.enabled {
                return;
            }

            self.records.clear();
            self.records.reserve(record_count);
            self.start_time = Instant::now();

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
                let duration = entry.time.duration_since(self.start_time).as_micros();

                // Format the string
                write!(w, "{{\"name\":\"{}\",\"ph\":\"{}\",\"ts\": {},\"tid\":{},\"cat\":\"\",\"pid\":0,\"args\":{{}}}}",
                    tag, type_tag, duration, stack.index)?;
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
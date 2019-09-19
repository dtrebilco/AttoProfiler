
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

pub mod internal {

    use std::sync::Mutex;
    use std::sync::Once;
    use std::io::Write;
    use std::io::BufWriter;
    use std::io;
    
    use std::time::Instant;
    use std::thread::{self, ThreadId};
    use std::collections::HashMap;

    enum TagType
    {
        Begin(&'static str),
        End,
        Complete(&'static str, u64) // A complete event holds a duration of the event
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
        time : Instant
    }

    impl ProfileScope {
        pub fn new(name: &'static str) -> ProfileScope {
            let thread_id = thread::current().id();            

            // Start as a begin tag
            let mut ret = ProfileScope { index : None, time : Instant::now() };            
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
                    if index < profile.records.len() {
                        let record = &mut profile.records[index];
                        if let TagType::Begin(name) = record.tag {
                            if self.time == record.time {
                                // If the time is different, it must have started in a different profile session
                                // Change the tag type to complete
                                let duration = Instant::now().duration_since(record.time).as_micros() as u64;                            
                                record.tag = TagType::Complete(name, duration);
                            }
                        }
                    }
                }
            }            
        }
    }

    pub fn profile_begin(tag : &'static str)
    {
        let thread_id = thread::current().id();
        if let Ok(ref mut profile) = get_profile() {
            profile.add_record(ProfileRecord { thread_id, tag : TagType::Begin(tag), time : Instant::now() });
        }            
    }

    pub fn profile_end()
    {
        let time = Instant::now(); // Always get time as soon as possible
        let thread_id = thread::current().id();
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
            profile.start_time = Instant::now();

            profile.enabled = true;
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
        end(&mut BufWriter::new(std::fs::File::create(filename)?))
    }

    pub fn end(w : &mut dyn Write) -> io::Result<()> {
        if let Ok(ref mut profile) = get_profile() {
            // Abort if already enabled
            if !profile.enabled {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }

            profile.enabled = false;

            // DT_TODO: Parhaps use thread::current().name() ?
            let mut thread_stack = HashMap::new();
            thread_stack.insert(thread::current().id(), Tags { index : 0, tags : vec!()});

            let mut first : bool = true;
            let mut clean_buffer : String = String::new();
            let mut duration_buffer : String = String::new();

            w.write(b"{\"traceEvents\":[\n")?;
            for entry in profile.records.iter()
            {
                // Assign a unique index to each thread                
                let new_id = thread_stack.len();
                let stack = thread_stack.entry(entry.thread_id).or_insert(Tags { index : new_id, tags : vec!()});

                let tag;
                let type_tag;
                duration_buffer.clear();
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
                        duration_buffer = format!("\"dur\":{},", d);
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
                let tag_time = entry.time.duration_since(profile.start_time).as_micros() as u64;

                // Format the string
                write!(w, "{{\"name\":\"{}\",\"ph\":\"{}\",\"ts\":{},\"tid\":{},{}\"pid\":0}}",
                    tag, type_tag, tag_time, stack.index, duration_buffer)?;
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

    fn get_profile() -> std::sync::LockResult<std::sync::MutexGuard<'static, ProfileData>> {
        static INIT : Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            GPROFILE.as_ref().unwrap_or_else(|| {std::hint::unreachable_unchecked()}).lock()
        }  
    }
}
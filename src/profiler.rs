
use std::io::BufWriter;
use std::io::Write;

pub fn begin(tag_count : usize ) {
    internal::begin(tag_count)
}

pub fn profile_begin(tag : &'static str){
    internal::profile_begin(tag)
}

pub fn profile_end(){
    internal::profile_end()
}

pub fn profile_scope(tag : &'static str) -> internal::ProfileScope {
    internal::ProfileScope::new(tag)
}

pub fn end(writer : &mut dyn Write) -> std::io::Result<()> {
    internal::end(writer)
}

pub fn end_to_file(filename : &str) -> std::io::Result<()> {
    end(&mut BufWriter::new(std::fs::File::create(filename)?))
}


mod internal {

    use std::sync::Mutex;
    use std::sync::Once;
    use std::io::Write;
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

        fn add_record(&mut self, record : ProfileRecord) {
            if !self.enabled || 
               self.records.len() >= self.records.capacity()
            {
                return;
            }
            self.records.push(record);
        }            
    }

    pub struct ProfileScope {
        name : &'static str,
        time : Instant, 
    }

    impl ProfileScope {
        pub fn new(name: &'static str) -> ProfileScope {
            ProfileScope {
                name, time : Instant::now(),
            }
        }
    }

    impl Drop for ProfileScope {
        fn drop(&mut self) {
            let thread_id = thread::current().id();
            if let Ok(ref mut profile) = get_profile() {
                if self.time < profile.start_time {
                    self.time = profile.start_time; // If this scope started before profiling started
                }
                let duration = Instant::now().duration_since(self.time).as_micros() as u64;                    
                profile.add_record(ProfileRecord { time : self.time, thread_id, tag : TagType::Complete(self.name, duration) });
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
                write!(w, "{{\"name\":\"{}\",\"ph\":\"{}\",\"ts\": {},\"tid\":{},\"cat\":\"\",\"pid\":0,{}\"args\":{{}}}}",
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

    fn get_profile() -> std::sync::TryLockResult<std::sync::MutexGuard<'static, ProfileData>> {
        static INIT : Once = Once::new();
        static mut GPROFILE : Option<Mutex<ProfileData>> = None;

        unsafe {
            INIT.call_once(|| {
                GPROFILE = Option::Some(Mutex::new(ProfileData::new()));
            });
            GPROFILE.as_ref().unwrap_or_else(|| {std::hint::unreachable_unchecked()}).try_lock()
        }  
    }
}
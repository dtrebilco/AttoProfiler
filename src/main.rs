use std::{thread, time};

mod oldlib;
mod profiler;

fn main() {

    profile_start!(100_000);
    {
        profile_scope!("Test scope");

        profile_begin!("Test area");
        thread::sleep(time::Duration::from_millis(10));
        profile_end!();
    }
    let _ = profile_finish_to_file!("test2.txt");

    open_trace_file!(".").unwrap();
    {
        trace_scoped!("complete","custom data":"main");
        trace_expr!("trace_expr", println!("trace_expr"));
        trace_begin!("duration");
        println!("trace_duration");
        trace_end!("duration");
    }
    close_trace_file!();
}

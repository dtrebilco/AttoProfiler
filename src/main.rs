
mod oldlib;
mod profiler;

fn main() {

    profiler::begin(100_000);

    profiler::profile_begin("Test area");
    profiler::profile_end();

    profiler::end_to_file("test2.txt");

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

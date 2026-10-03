fn main() {
    use righttype::{dict, layout, policy::InputLayout, sim::Engine};
    for t in [
        "ก.ค. ",
        "วันที่ 12 ก.ค. 2567 ",
        "ส.ส. ",
        "ดร.สมชาย ",
        "กทม. ",
        "พ.ศ. 2567 ",
        "สวัสดีครับ ",
    ] {
        let mut e = Engine::new(dict::english(), dict::thai(), InputLayout::UsQwerty);
        let typed = layout::th_to_en(t);
        for c in typed.chars() {
            if c == ' ' {
                e.boundary(' ')
            } else {
                e.key(c)
            }
        }
        println!("{t:?} typed {typed:?} -> {:?}", e.screen);
    }
}

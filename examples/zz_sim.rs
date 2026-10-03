fn main() {
    use righttype::{dict, layout, policy::InputLayout, sim::Engine};
    let lines = [
        "ส่งไฟล์มาให้หน่อยนะครับพรุ่งนี้ต้องใช้แล้ว ",
        "วันนี้ประชุมเรื่องงบประมาณปีหน้าช่วยเตรียมเอกสารด้วย ",
        "ตามที่ได้รับมอบหมายจากที่ประชุมให้ดำเนินการจัดทำรายงานสรุปผลการดำเนินงานประจำปี ",
        "การพัฒนาระบบสารสนเทศเพื่อการบริหารจัดการองค์กรอย่างมีประสิทธิภาพ ",
        "นักเรียนทุกคนต้องส่งการบ้านภายในวันศุกร์นี้ ",
        "ผมคิดว่าเราควรจะเริ่มทำโครงการนี้ตั้งแต่สัปดาห์หน้า ",
        "บทคัดย่องานวิจัยนี้มีวัตถุประสงค์เพื่อศึกษาปัจจัยที่ส่งผลต่อความพึงพอใจของผู้ใช้บริการ ",
    ];
    let mut ok = 0;
    for t in lines {
        let mut e = Engine::new(dict::english(), dict::thai(), InputLayout::UsQwerty);
        let typed = layout::th_to_en(t);
        for c in typed.chars() {
            if c == ' ' {
                e.boundary(' ')
            } else {
                e.key(c)
            }
        }
        let good = e.screen == t;
        ok += good as usize;
        println!(
            "{}\n  want {t}\n  got  {}\n  {:?}\n",
            if good { "OK" } else { "BAD" },
            e.screen,
            e.counters
        );
    }
    println!("{ok}/{}", lines.len());
}

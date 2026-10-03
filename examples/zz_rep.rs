fn main() {
    use righttype::{dict, layout, repair};
    let thai = [
        "ส่งไฟล์มาให้หน่อยนะครับพรุ่งนี้ต้องใช้แล้ว",
        "วันนี้ประชุมเรื่องงบประมาณปีหน้า ช่วยเตรียมเอกสารด้วย",
        "ตามที่ได้รับมอบหมายจากที่ประชุมเมื่อวันที่ 12 ก.ค. 2567 ให้ดำเนินการ",
        "กรุงเทพฯ เป็นเมืองหลวง ส.ส. และ ดร.สมชาย มาประชุม",
        "ผมใช้ Python กับ LaTeX เขียนรายงานครับ",
        "สวัสดีครับ ยินดีที่ได้รู้จัก",
    ];
    for t in thai {
        let typed = layout::th_to_en(t);
        let r = repair::repair(&typed, dict::english(), dict::thai());
        println!(
            "typed: {typed}\nfixed: {}\nwant : {t}\nok={}\n",
            r.text,
            r.text == t
        );
    }
}

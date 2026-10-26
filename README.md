<div align="center">

# RightType

### พิมพ์ผิดภาษา แก้ให้เอง

แปลงข้อความที่พิมพ์ผิดคีย์บอร์ด ไทย ↔ อังกฤษ กลับให้ถูกต้อง โดยไม่ต้องลบแล้วพิมพ์ใหม่

~~แนพพำแะ~~ → **correct**  ·  ~~l;ylfu~~ → **สวัสดี**

[**ดาวน์โหลด**](https://github.com/k-affan-th/RightType/releases/latest) ·
[คู่มือฉบับเต็ม](docs/GUIDE.md) ·
[Changelog](CHANGELOG.md)

ฟรี · โอเพนซอร์ส (MIT / Apache-2.0) · Windows · เขียนด้วย Rust

</div>

---

*English: RightType fixes text typed in the wrong keyboard layout (Thai typed while
English is active, or the other way round) without retyping. Free, open source,
no network code.*

## ทำอะไรได้

| | |
| --- | --- |
| **แก้ให้อัตโนมัติ** | พิมพ์ต่อได้เลย RightType แปลงคำที่ผิดภาษาและสลับภาษาให้ หรือจะให้แค่แนะนำแล้วกด `Tab` ก็ได้ |
| **แก้เองด้วยปุ่มเดียว** | กด `Shift`+`Backspace` แก้คำล่าสุด กดซ้ำเพื่อย้อนไปทีละคำ (สูงสุด 8 คำ) |
| **ซ่อมทั้งย่อหน้า** | วางข้อความที่พิมพ์ผิดภาษาไว้ทั้งก้อน แล้วคัดลอกฉบับที่ถูกกลับไป |
| **ตั้งแยกตามแอป** | เช่น ใน VS Code ไม่แตะชื่อตัวแปร ส่วน LINE ให้แก้อัตโนมัติ |
| **มากกว่าแค่แก้ภาษา** | Snippets, สูตรคณิตศาสตร์, ตัวอักษรพิเศษ, ฝึกพิมพ์ Kedmanee / Pattachote / Manoonchai |

| ตั้งค่า | แต่ละแอปตั้งโหมดได้ | Command palette |
| --- | --- | --- |
| ![Settings](docs/img/settings.png) | ![Apps](docs/img/apps-dark.png) | ![Command palette](docs/img/palette-dark.png) |

## ความเป็นส่วนตัว

โปรแกรมคีย์บอร์ดเห็นทุกอย่างที่คุณพิมพ์ RightType จึงออกแบบให้ไว้ใจได้:

- **ไม่มีโค้ดเครือข่าย** ไม่มี telemetry ตรวจได้จาก `Cargo.lock`
- **ไม่บันทึกสิ่งที่พิมพ์ลงดิสก์** เก็บแค่คำที่กำลังพิมพ์ในหน่วยความจำ แล้วล้างทิ้ง
- **ปิดตัวเอง** ในช่องรหัสผ่าน, password manager, กระเป๋าคริปโต, terminal, remote desktop และเกมเต็มจอ
- **ไม่ต้องใช้สิทธิ์ admin**

รายละเอียดอยู่ใน [Privacy & security](docs/GUIDE.md#privacy--security-built-for-sensitive-typing)
และ [Threat model](docs/THREAT_MODEL.md)

## ติดตั้ง

1. ดาวน์โหลด `RightType-<เวอร์ชัน>-setup.exe` จากหน้า
   [Releases ล่าสุด](https://github.com/k-affan-th/RightType/releases/latest)
2. ดับเบิลคลิก ถ้า Windows ขึ้น "Windows protected your PC" ให้กด **More info → Run anyway**
   (ไฟล์ยังไม่ได้ลงลายเซ็นดิจิทัล)
3. RightType อยู่ที่ system tray มุมขวาล่าง (ไอคอน **Aก**) คลิกเพื่อเปิดเมนู

มี winget? `winget install k-affan-th.RightType`

Windows on ARM, portable zip, ตรวจ SHA256 และวิธีถอนการติดตั้ง:
ดู [Install ในคู่มือฉบับเต็ม](docs/GUIDE.md#install-no-building-needed)

## อ่านต่อ

| | |
| --- | --- |
| [คู่มือฉบับเต็ม](docs/GUIDE.md) | ปุ่มลัดทั้งหมด, โหมด Auto / Suggest / Manual, เครื่องมือคีย์บอร์ด, วิธี build |
| [Undo](docs/UNDO.md) | ย้อนการแก้ไขทำงานอย่างไร |
| [Typing benchmark](docs/TYPING_BENCHMARK.md) | วัดความแม่นและอัตราแก้คำที่พิมพ์ถูกอยู่แล้วให้ผิด |
| [Threat model](docs/THREAT_MODEL.md) | RightType ป้องกันอะไร และอะไรที่ยังป้องกันไม่ได้ |
| [Changelog](CHANGELOG.md) | สิ่งที่เปลี่ยนในแต่ละเวอร์ชัน |

## License

MIT หรือ Apache-2.0 ตามที่คุณเลือก — ดู [LICENSE-MIT](LICENSE-MIT) และ [LICENSE-APACHE](LICENSE-APACHE)

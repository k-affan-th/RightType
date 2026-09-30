@echo off
chcp 65001 >nul
cd /d "%~dp0"
echo ============================================================
echo  RightType self-test / ทดสอบ RightType บนเครื่องนี้
echo ============================================================
echo.
echo  ประมาณ 10-15 นาที โปรแกรมจะพิมพ์ใน Notepad, Edge และ Chrome เอง
echo  ระหว่างนี้ห้ามแตะคีย์บอร์ดและเมาส์
echo  RightType ที่เปิดอยู่จะถูกปิด เปิดใหม่ได้หลังทดสอบเสร็จ
echo  (ไม่แตะการตั้งค่าหรือคำที่เรียนรู้ของคุณ ใช้โฟลเดอร์ชั่วคราวแยก)
echo.
echo  About 10-15 minutes. Do not touch the keyboard or mouse.
echo.
if not exist "%~dp0righttype.exe" (
  echo  righttype.exe is missing: unzip the whole self-test zip first.
  pause
  exit /b 1
)
where py >nul 2>nul
if errorlevel 1 (
  echo  ต้องมี Python ก่อน: เปิด PowerShell แล้วพิมพ์
  echo      winget install Python.Python.3.12
  echo  จากนั้นดับเบิลคลิกไฟล์นี้อีกครั้ง
  pause
  exit /b 1
)
echo  เริ่มใน 10 วินาที... (กด Ctrl+C เพื่อยกเลิก)
timeout /t 10 >nul
py -3 -m pip install --quiet --user "pywinauto>=0.6.9" "mss>=10"
if errorlevel 1 (
  echo  ติดตั้งเครื่องมือทดสอบไม่สำเร็จ
  pause
  exit /b 1
)
set "RIGHTTYPE_EXE=%~dp0righttype.exe"
set "PYTHONIOENCODING=utf-8"
py -3 ci_sweep.py
echo.
echo ============================================================
echo  เสร็จแล้ว ส่ง 2 ไฟล์นี้ให้ผู้พัฒนา / Done. Send these two files:
echo    %~dp0RightType-test-report.txt
echo    %~dp0RightType-test-trace.txt
echo ============================================================
pause

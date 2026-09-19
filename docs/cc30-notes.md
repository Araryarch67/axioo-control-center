# Control Center 3.0 (Windows) — catatan bedah installer

Sumber: installer InstallShield di mesin (`setup.exe` + `data1.cab` 10MB +
`data2.cab` 338MB, 392 file, `oem.ini` versi `v7.031`).
Dibuka dengan `unshield x data1.hdr` (392 → 351 file; `.config` Insyde
terenkripsi, tidak bisa dibaca statis).

## Arsitektur (relevan untuk Linux)

- App modular per fitur (bundle UWP): `FanSpeedSetting` 6.91,
  `KB_Perkey` 6.26, `CPU_OC`, `GPUOverclocking`, `Flexikey`, `FnKey`,
  `BatteryUtility`, `EnergySave`, induk `CC30`.
- Jembatan driver: `InsydeDCHU.dll` + `clevo_acpi`/`clevo_wmi` sys
  (ada di `DefaultComponent`) — setara `clevo-drivers` kita.
- Gating model: `GetProductdll.dll` (`CheckClevo`, `GetProductID`,
  `GetProductID_PCI`) — setara `devices.rs` kita.

## Kipas

- App .NET (`FanSpeedSetting.exe`) bicara via **`Clevo_WMI_Command`**
  (WMI, bukan port I/O langsung): tabel `UC/SP_FanTable_CPU/GPU1/GPU2`,
  mode Silent + Custom. Jalur Linux kita (EC `0x62/0x66` mentah) adalah
  lapisan di bawah WMI yang sama — register yang dituju identik.

## Keyboard

- Komponen bernama `KB_Perkey` + `perkey_api.dll` (`InitPerkeyIo`, port
  I/O) + UI `RB_Perkey_*` + `SetLightBarData62` + dukungan
  `LightBar_x170`. Artinya **protokol per-key ada di codebase app** —
  status "tidak ada mode per-key" di `docs/per-key-rgb.md` berlaku untuk
  yang tampil di firmware/mesin ini, bukan untuk app secara umum.
  Per-key via EC di X560WNR tetap belum terbukti (butuh capture saat
  mode itu aktif, yang tidak ada di mesin ini).

## oem.ini (template generik, bukan per-model)

- Mode fan (`0 Auto 1 Max 3 Silent 5 MAXQ 6 Custom`), mode situasi
  (`Quiet/power saving/performance/Entertainment`), default LED
  (`KbLeft/Mid/Right`), `SupportFanSpeedOffset`, `SupportXTUFanTable`.
  Bagian CPU/GPU OC masih template komentar (i7-9700K/GTX1060 era).

## Belum dibedah (butuh decompiler .NET, mis. ilspyc)

- ID method WMI + byte EC persis di dalam `FanSpeedSetting.exe` /
  `LedKeyboardSetting.exe` dan tabel kurva bawaan per model.

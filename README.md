# Device Battery

Ứng dụng desktop Tauri 2 + React + TypeScript để theo dõi pin thiết bị ngoại vi. Rust đọc thông tin từ hệ điều hành, lưu lịch sử và gửi thông báo; React hiển thị trạng thái. Bản đầu ưu tiên Linux/Ubuntu.

## Chạy ứng dụng

```sh
pnpm install
pnpm tauri dev
```

Cần Rust và các [thư viện phát triển Tauri cho Linux](https://v2.tauri.app/start/prerequisites/#linux). Phiên bản Node cần đáp ứng yêu cầu của Vite trong `package.json`.

```sh
pnpm dev          # Giao diện web, không đọc phần cứng
pnpm build        # Kiểm tra TypeScript và build giao diện
pnpm tauri build # Build ứng dụng desktop
```

Trình duyệt mở ở chế độ preview trống. Nhấn **Xem bản demo** để xem dữ liệu minh họa; demo không gửi thông báo, không ghi vào lịch sử hoặc cấu hình desktop. Dữ liệu demo không chứng minh thiết bị được hỗ trợ.

## Những gì bản này làm được

- Phát hiện thiết bị HID qua sysfs, gộp các interface của cùng thiết bị.
- Đọc pin ngoại vi mà Linux công bố qua `/sys/class/power_supply`; không lấy pin laptop làm pin chuột/bàn phím.
- Đọc Bluetooth qua BlueZ `Device1` và `Battery1` khi dịch vụ, quyền truy cập và thông tin pin có sẵn. Máy cần lệnh `busctl` (thuộc systemd); thiếu BlueZ không chặn nguồn HID/sysfs.
- Hiển thị pin, sạc và kết nối dưới dạng có thể chưa biết. Receiver USB xuất hiện không có nghĩa là ngoại vi phía sau đang kết nối; không suy đoán 2.4 GHz từ USB.
- Tự quét bằng Rust, mặc định 5 giây; điều chỉnh 5–60 giây trong Cài đặt. Cửa sổ ẩn không dừng theo dõi.
- Lịch sử pin cục bộ trong 24 giờ, có giới hạn số mẫu và thiết bị. Chỉ ghi mẫu pin hợp lệ; không biến `Unknown` thành 0%.
- Cảnh báo dưới ngưỡng 20% (đổi được), bỏ qua thiết bị đang sạc hoặc đã xác nhận ngắt kết nối. Thông báo mặc định tắt, bật trong Cài đặt. Không gửi lặp lại mỗi lần quét; pin hồi phục tới ngưỡng sẽ cho phép cảnh báo lần tiếp theo.
- Tray có menu mở/ẩn/thoát. Đóng cửa sổ sẽ thoát ứng dụng; dùng menu tray để ẩn và tiếp tục theo dõi. Khả năng hiển thị tray phụ thuộc desktop environment.

Cấu hình và lịch sử nằm trong `monitor.json` ở thư mục app data của Tauri cho `com.hy.bluetooth-adapter` (thường là `~/.local/share/com.hy.bluetooth-adapter` trên Linux). Lỗi lưu hoặc gửi thông báo được hiển thị trong giao diện.

**Thời gian theo dõi** là thời gian phiên ứng dụng, không phải thời lượng pin còn lại. Không có dữ liệu đủ tin cậy để ước lượng số giờ sử dụng của từng mẫu thiết bị.

## Giới hạn hỗ trợ

| Nguồn | Trạng thái |
| --- | --- |
| Linux HID + power_supply | Có triển khai đọc thật, chỉ lấy dữ liệu driver công bố |
| Linux BlueZ Battery1 | Có triển khai, cần BlueZ và thiết bị có cung cấp pin |
| Windows HID/Windows APIs | Chưa triển khai; hiển thị provider chưa hỗ trợ |
| Protocol riêng của Logitech/Razer/F17 Pro/... | Chưa triển khai; hiển thị `Unknown` khi nguồn hệ điều hành không có pin |
| Polling rate, firmware, thời gian pin còn lại | Chưa đọc/ước lượng |

Ứng dụng chỉ đọc metadata và thông tin pin, không gửi HID feature report hoặc thay đổi firmware. Không thể đảm bảo lấy được pin của một receiver 2.4 GHz chỉ vì nhận diện được tên USB.

Để xem dữ liệu mà máy thực sự công bố, chạy chẩn đoán chỉ đọc (không mở cửa sổ, không gửi thông báo):

```sh
cargo run --manifest-path src-tauri/Cargo.toml --example scan_devices
```

Thiết bị USB không có serial được nhận diện theo cổng vật lý; chuyển cổng có thể tạo ID/lịch sử mới.

## Cấu trúc

- `src-tauri/src/providers.rs`: discovery, ánh xạ dữ liệu hệ điều hành sang `Device`, tình trạng từng nguồn.
- `src-tauri/src/monitor.rs`: vòng quét, settings, lưu lịch sử và chống lặp cảnh báo.
- `src-tauri/src/lib.rs`: Tauri commands/events, notification plugin và tray.
- `src/types.ts`: contract giao diện; `battery`, `charging`, `connected` có thể là `null`.
- `src/useMonitor.ts`: kết nối commands/events, preview và demo tách khỏi dữ liệu thật.
- `src/App.tsx`: dashboard, lịch sử và cài đặt.

Để thêm một hãng, triển khai adapter riêng và ánh xạ về contract `Device`; không đưa protocol phần cứng vào React. Giữ `null` khi chưa có bằng chứng và cung cấp trạng thái nguồn đọc khi lỗi.

## Kiểm tra

```sh
pnpm build
pnpm test:ui
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
git diff --check
```

Browser tests dùng Google Chrome đã cài trên máy (`channel: "chrome"`), khởi chạy Vite trên cổng 1420. Chúng kiểm tra preview/demo và tương tác UI; unit test Rust kiểm tra logic provider/monitor. Các kiểm tra này không thay thế thử nghiệm pin thật trên từng model, thông báo hệ thống hoặc Windows.

Tham khảo: [Linux power_supply](https://www.kernel.org/doc/html/latest/power/power_supply_class.html), [BlueZ Battery API](https://bluez.readthedocs.io/en/latest/battery-api/), [Tauri tray](https://v2.tauri.app/learn/system-tray/), [Tauri notifications](https://v2.tauri.app/plugin/notification/).

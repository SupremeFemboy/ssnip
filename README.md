# ssnip

> ⚠️ **100% AI-generated slop:** This code was entirely architected, written, and refactored by AI agents under the supervision of a Supreme Femboy. Use at your own risk.

---

## English

A fast, lightweight Wayland screenshot utility written in Rust using `wayland-client` and `memmap2`. It captures screens without compositor delay, opens an interactive layer-shell surface on `Layer::Overlay` with customizable dimming and live selection frames, and isolates windows from overlapping floating windows while preserving true translucent desktop backgrounds.

The output encoding format is automatically determined by the file extension (`.webp`, `.png`, `.jpg`/`.jpeg`, `.bmp`) with fine-grained per-format compression options (`[webp]`, `[jpeg]`, `[png]`). Features single-instance execution protection via kernel `flock`, native D-Bus notifications, direct Wayland clipboard integration (`image/png`), automatic `YYYY-MM` directory organization, and format-preserving TOML configuration migration.

### Features

- **Native Wayland Screencopy & SHM Buffers.** Directly utilizes `zwlr_screencopy_v1` with anonymous file descriptors (`memfd_create`) mapped into memory via `memmap2::MmapMut`. Captures pixels with zero pipeline lag before rendering the overlay.
- **Instant Overlay Dismissal.** The moment selection is confirmed, layer-shell surfaces are destroyed and flushed immediately, eliminating any visual freezing while image encoding and disk I/O take place in the background.
- **Interactive Selection & Dimming.** Opens a fullscreen layer-shell surface on `zwlr_layer_shell_v1` with exclusive keyboard grab. The display is smoothly dimmed with custom colors and opacity; dragging left mouse button restores original bright pixels inside the selection frame.
- **Window Detection & Overlap Isolation.** Automatically queries window hierarchies in Hyprland (including 0.57+ socket IPC with `hyprctl` fallback) and Sway (`SWAYSOCK`). Hovering over windows highlights them, and clicking captures that exact window. If floating windows overlap the targeted window, toplevel buffers patch only the occluded regions, keeping true translucent wallpapers and backgrounds intact.
- **Format-Driven Encoding by File Extension.** Output format is automatically selected based on the extension in `output.filename`:
  - `.webp`: Pure Rust / Google `libwebp` encoding with dedicated `[webp]` controls (`lossless`, `quality`, `method`, `exact`).
  - `.jpg` / `.jpeg`: High-fidelity JPEG encoding via `[jpeg]` (`quality = 100` eliminates block and ringing compression artifacts on text).
  - `.png`: Deflate compression via `[png]` (`compression = "fast" | "default" | "best"`).
  - `.bmp`: Uncompressed bitmap encoding.
- **Native Wayland Clipboard Integration.** Automatically copies screenshots to the Wayland clipboard as standard `image/png` via `wl-clipboard-rs` and `wl-copy`. If saving to WebP/JPEG, in-memory PNG bytes are streamed directly to clipboard workers without re-reading compressed files from disk.
- **Native D-Bus Desktop Notifications.** Sends clean, transient desktop notifications directly over the D-Bus session bus (`org.freedesktop.Notifications`) without spawning external binaries. Includes screenshot thumbnail and expires after exactly 1 second.
- **Single-Instance Lock via `flock`.** Guarantees that only one instance of `ssnip` can run simultaneously. Subsequent invocations exit silently in ~2 ms without cluttering the screen or conflicting with active captures.
- **Automatic Date Categorization (`YYYY-MM`).** Automatically creates and categorizes screenshots into monthly subdirectories (e.g. `~/Pictures/Screenshots/2026-11/`) inside the configured storage path.
- **Non-Destructive TOML Migration.** Uses `toml_edit` to automatically append newly introduced configuration parameters and sections into existing user configuration files without overwriting custom paths, colors, or comments.

### Architecture

```
src/
├── main.rs          # layer-shell event loop, pointer & keyboard dispatch, surface lifecycle
├── config.rs        # TOML config loader, format options, format-preserving migration
├── render.rs        # frame rendering, dimming, border math, multi-format encoders
├── window.rs        # Hyprland & Sway window discovery, overlap detection, toplevel capture
├── clipboard.rs     # native Wayland clipboard daemon integration (image/png)
├── notification.rs  # native D-Bus desktop notifications (org.freedesktop.Notifications)
├── color.rs         # universal color parser (hex, rgb, rgba, arrays) and alpha blending
├── shm.rs           # memfd_create shared memory buffers via memmap2
└── lock.rs          # single-instance flock mechanism for immediate duplicate exit
```

1. **Capture & Precomputation:** When launched, `ssnip` captures the monitor via `zwlr_screencopy_v1` into `memmap2::MmapMut`. The dimmed framebuffer is precomputed once, enabling fluid, zero-copy double-buffered overlay updates during mouse drags.
2. **Selective Patching:** When an occluded window is selected, `grim -T` acquires the window's isolated surface. `ssnip` calculates exact geometric intersections with foreground floating windows and patches only the obscured rectangles, preserving native blur, shadows, and desktop backgrounds elsewhere.
3. **Optimized Encoders:** WebP lossless encoding utilizes multi-threading (`thread_level = 1`) and balanced effort presets (`method = 1`), preventing CPU hangs on high-resolution displays.

### Configuration

Paths adhere to the XDG Base Directory specification:

| File | Path |
|---|---|
| `config.toml` | `$XDG_CONFIG_HOME/ssnip/config.toml` (default `~/.config/ssnip/config.toml`) |

If `config.toml` does not exist on startup, it is automatically created with default settings. Missing parameters in older configs are appended automatically without altering user values:

```toml
# Storage settings
[output]
# Base directory where screenshots are saved (supports '~' and strftime patterns)
# Subdirectories formatted as 'YYYY-MM' are automatically created inside
dir = "~/Pictures/Screenshots"

# Filename format supporting strftime tokens (%Y, %m, %d, %H, %M, %S)
# The file extension (.webp, .png, .jpg, .jpeg, .bmp) selects the encoder
filename = "Screenshot_%Y-%m-%d_%H-%M-%S.webp"

# Composite cursor onto the captured image
capture_cursor = false

# Copy captured screenshot to Wayland clipboard
copy_to_clipboard = true

# Dim overlay settings
[dim]
# Screen dimming color and opacity.
# Supports hex ("#00000080", "#0008"), rgb("rgb(0,0,0)"), rgba("rgba(0,0,0,0.5)"), or [r, g, b, a]
color = "#00000080"

# Selection border settings
[border]
# Border color (hex, rgb, rgba, or array)
color = "#ffffff"

# Border thickness in pixels
width = 2

# WebP compression options (active when filename ends in .webp)
[webp]
# Lossless mode (true = bit-exact, 0 artifacts; false = lossy)
lossless = true

# Quality factor (0.0 - 100.0). In lossy mode: visual quality; in lossless mode: compression effort
quality = 90.0

# Compression method (0 = fastest / minimal CPU, 6 = slowest / highest compression)
method = 1

# Preserve exact RGB values under transparent pixels
exact = true

# JPEG compression options (active when filename ends in .jpg or .jpeg)
[jpeg]
# JPEG quality (1 - 100). Default is 100 for artifact-free text rendering
quality = 100

# PNG compression options (active when filename ends in .png)
[png]
# PNG compression level: "fast", "default", or "best"
compression = "default"
```

### Controls & Keybindings

| Key / Input | Action |
|---|---|
| `Left Click (on window)` | Select and capture clicked window (occlusion-isolated) |
| `Left Click (on background)` | Select and capture full screen |
| `LMB Drag & Release` | Select arbitrary rectangular region and save |
| `Enter` / `Numpad Enter` | Confirm current selection and save |
| `Esc` | Cancel and exit without saving |

### CLI Usage & Integration Examples

#### Standard execution:
```bash
ssnip
```

#### With custom configuration file:
```bash
ssnip -c ~/.config/ssnip/custom.toml
```

#### CLI flags for quick overrides:
```bash
# Capture cursor on screenshot
ssnip --cursor

# Do not capture cursor
ssnip --no-cursor

# Enable / disable copying to clipboard
ssnip --clipboard
ssnip --no-clipboard

# Override JPEG / lossy quality (1-100)
ssnip -q 95
```

#### Hyprland Keybinding (`hyprland.conf`):
```ini
# Area / Window selection
bind = , Print, exec, ssnip

# Fullscreen with cursor capture
bind = SHIFT, Print, exec, ssnip --cursor
```

### Build & run

```bash
git clone https://github.com/RustBlyat/ssnip
cd ssnip
cargo build --release
./target/release/ssnip
```

### Installing from source (Arch Linux)

An AUR-style `PKGBUILD` is included and builds straight from the git repository:

```bash
git clone https://github.com/RustBlyat/ssnip
cd ssnip
makepkg -si
```

The package installs the binary to `/usr/bin/ssnip`. Runtime dependencies are `glibc`, `gcc-libs`, `wayland`, and `libwebp`.

### Notes & limitations

- Requires a Wayland compositor implementing `zwlr_layer_shell_v1` and `zwlr_screencopy_v1` (e.g. Hyprland, Sway, River, Wayfire, Labwc). Does not run on X11 or GNOME/Mutter (which intentionally omit layer-shell support).
- Isolated window capture requires `grim` and window-management IPC (Hyprland UNIX socket or Sway IPC). If unavailable, standard rectangular screencopy is seamlessly used as fallback.
- Wayland clipboard data is offered as standard `image/png` for universal compatibility across desktop applications (browsers, messengers, image editors).

---

## Русский

Быстрый, легковесный скриншотер для Wayland, написанный на Rust с использованием `wayland-client` и `memmap2`. Захватывает экран без задержек композитора, открывает интерактивный слой на `Layer::Overlay` с настраиваемым затемнением и рамкой выделения, а также изолирует выбранные окна от перекрывающих их плавающих окон с сохранением настоящего прозрачного фона рабочего стола.

Формат сохранения автоматически определяется расширением файла (`.webp`, `.png`, `.jpg`/`.jpeg`, `.bmp`) с отдельными группами тонкой настройки сжатия (`[webp]`, `[jpeg]`, `[png]`). Поддерживает защиту от повторного запуска через `flock`, нативные уведомления D-Bus, прямую интеграцию с буфером обмена Wayland (`image/png`), автоматическую сортировку по папкам `YYYY-MM` и аккуратную миграцию TOML-конфигурации.

### Возможности

- **Нативный Wayland Screencopy и SHM-буферы.** Использует протокол `zwlr_screencopy_v1` с анонимными файловыми дескрипторами (`memfd_create`), отображаемыми в память через `memmap2::MmapMut`. Захват кадра происходит мгновенно до отрисовки интерфейса.
- **Моментальное закрытие оверлея.** При подтверждении выбора оверлейные поверхности layer-shell немедленно уничтожаются и сбрасываются в сокет, благодаря чему экран пользователя размораживается за 0 мс, пока сохранение и сжатие происходят в фоне.
- **Интерактивное выделение и затемнение.** Разворачивает полноэкранный слой на `zwlr_layer_shell_v1` с монопольным перехватом фокуса. Экран плавно затемняется заданным цветом; растягивание рамки ЛКМ восстанавливает оригинальную яркость пикселей внутри выделения.
- **Распознавание окон и изоляция перекрытий.** Автоматически запрашивает геометрию окон в Hyprland (включая IPC сокетов версии 0.57+ с отказоустойчивым fallback на `hyprctl`) и Sway (`SWAYSOCK`). Наведение подсвечивает окно, а клик выбирает его. Если поверх окна находятся другие плавающие окна, буфер toplevel закрывает только области перекрытия, сохраняя настоящие обои и полупрозрачность.
- **Управление энкодером через расширение файла.** Формат сохранения автоматически выбирается по расширению в `output.filename`:
  - `.webp`: Сжатие через pure-Rust / Google `libwebp` с тонкими настройками `[webp]` (`lossless`, `quality`, `method`, `exact`).
  - `.jpg` / `.jpeg`: Высококачественный JPEG через `[jpeg]` (`quality = 100` исключает артефакты и ореолы вокруг текста).
  - `.png`: Deflate-сжатие через `[png]` (`compression = "fast" | "default" | "best"`).
  - `.bmp`: Несжатый растровый формат BMP.
- **Нативная интеграция с буфером обмена Wayland.** Созданный снимок автоматически копируется в буфер обмена Wayland как стандартный `image/png` через `wl-clipboard-rs` и `wl-copy`. При сохранении в WebP/JPEG байты PNG генерируются напрямую в памяти без повторного чтения сжатых файлов с диска.
- **Нативные уведомления через D-Bus.** Отправляет легковесные уведомления напрямую в шину D-Bus (`org.freedesktop.Notifications`) без запуска внешних утилит. Уведомление содержит миниатюру снимка и исчезает ровно через 1 секунду.
- **Защита от повторного запуска через `flock`.** Гарантирует запуск строго одного экземпляра `ssnip`. Повторные вызовы мгновенно и тихо завершаются (~2 мс), не конфликтуя с активным процессом выделения.
- **Сортировка по датам (`YYYY-MM`).** Автоматически создает подкаталоги формата текущего года и месяца (например, `~/Pictures/Screenshots/2026-11/`) внутри базовой папки.
- **Бережная миграция TOML-конфигурации.** Использует `toml_edit` для автоматического добавления новых параметров в существующий файл конфигурации без изменения пользовательских путей, цветов и комментариев.

### Архитектура

```
src/
├── main.rs          # цикл событий layer-shell, обработка указателя и клавиатуры, жизненный цикл поверхностей
├── config.rs        # загрузка конфигурации TOML, параметры форматов, формат-сохраняющая миграция
├── render.rs        # отрисовка кадра, затемнение, расчет границ, энкодеры форматов
├── window.rs        # поиск окон в Hyprland и Sway, расчет перекрытий, toplevel-захват
├── clipboard.rs     # интеграция с демоном буфера обмена Wayland (image/png)
├── notification.rs  # нативные уведомления D-Bus (org.freedesktop.Notifications)
├── color.rs         # универсальный парсер цвета (hex, rgb, rgba, массивы) и альфа-блендинг
├── shm.rs           # разделяемая память memfd_create через memmap2
└── lock.rs          # механизм flock для мгновенного выхода дубликатов
```

1. **Захват и предварительный расчет:** При запуске `ssnip` захватывает кадр экрана через `zwlr_screencopy_v1` в память `memmap2::MmapMut`. Затемненное изображение рассчитывается один раз, что обеспечивает плавную отрисовку рамки с двойной буферизацией без задержек.
2. **Бесшовная вставка поверхностей:** Если окно перекрыто другими окнами, утилита запрашивает чистую поверхность окна через `grim -T`, вычисляет точные области пересечения с перекрывающими окнами и замещает только эти зоны, оставляя фоновые обои и прозрачность нетронутыми.
3. **Оптимизированное сжатие:** В WebP lossless по умолчанию задействовано многопоточное кодирование (`thread_level = 1`) и сбалансированный профиль скорости (`method = 1`), что устраняет многосекундные зависания на экранах высокого разрешения.

### Конфигурация

Пути соответствуют спецификации XDG Base Directory:

| Файл | Путь |
|---|---|
| `config.toml` | `$XDG_CONFIG_HOME/ssnip/config.toml` (по умолчанию `~/.config/ssnip/config.toml`) |

При первом запуске файл конфигурации создаётся автоматически. Недостающие параметры аккуратно дописываются в существующий конфиг без сброса настроек пользователя:

```toml
# Storage settings
[output]
# Базовая директория для сохранения (поддерживает '~' и шаблоны strftime)
# Внутри автоматически создаются папки формата 'YYYY-MM'
dir = "~/Pictures/Screenshots"

# Шаблон имени файла с токенами strftime (%Y, %m, %d, %H, %M, %S)
# Расширение файла (.webp, .png, .jpg, .jpeg, .bmp) управляет выбором энкодера
filename = "Screenshot_%Y-%m-%d_%H-%M-%S.webp"

# Захватывать ли курсор мыши на скриншоте
capture_cursor = false

# Копировать ли созданный снимок в буфер обмена Wayland
copy_to_clipboard = true

# Dim overlay settings
[dim]
# Цвет и прозрачность затемнения экрана.
# Поддерживает hex ("#00000080", "#0008"), rgb("rgb(0,0,0)"), rgba("rgba(0,0,0,0.5)") или [r, g, b, a]
color = "#00000080"

# Selection border settings
[border]
# Цвет рамки выделения (hex, rgb, rgba или массив)
color = "#ffffff"

# Толщина рамки в пикселях
width = 2

# WebP compression options (активны, если расширение .webp)
[webp]
# Без потерь (true = попиксельная точность, 0 артефактов; false = с потерями)
lossless = true

# Качество (0.0 - 100.0). В режиме lossy: визуальное качество; в lossless: степень сжатия
quality = 90.0

# Метод сжатия (0 = быстрый / минимальная нагрузка на CPU, 6 = максимальное сжатие)
method = 1

# Сохранять точные значения RGB под прозрачными пикселями
exact = true

# JPEG compression options (активны, если расширение .jpg или .jpeg)
[jpeg]
# Качество JPEG (1 - 100). По умолчанию 100 для кристально четкого текста без артефактов
quality = 100

# PNG compression options (активны, если расширение .png)
[png]
# Степень сжатия PNG: "fast", "default" или "best"
compression = "default"
```

### Управление

| Клавиша / Ввод | Действие |
|---|---|
| `Клик ЛКМ (по окну)` | Выбрать и сохранить выбранное окно (без перекрывающих окон) |
| `Клик ЛКМ (по фону)` | Сохранить полный экран |
| `Зажать и потянуть ЛКМ` | Выделить прямоугольную область произвольного размера |
| `Enter` / `Numpad Enter` | Подтвердить текущее выделение и сохранить |
| `Esc` | Отмена и закрытие без сохранения |

### Примеры использования и CLI

#### Обычный запуск:
```bash
ssnip
```

#### С указанием пользовательского файла конфигурации:
```bash
ssnip -c ~/.config/ssnip/custom.toml
```

#### Флаги командной строки:
```bash
# Включить захват курсора
ssnip --cursor

# Отключить захват курсора
ssnip --no-cursor

# Включить / отключить копирование в буфер обмена
ssnip --clipboard
ssnip --no-clipboard

# Переопределить качество сжатия (1-100)
ssnip -q 95
```

#### Настройка горячих клавиш в Hyprland (`hyprland.conf`):
```ini
# Выбор области или окна
bind = , Print, exec, ssnip

# Скриншот с захватом курсора
bind = SHIFT, Print, exec, ssnip --cursor
```

### Сборка и запуск

```bash
git clone https://github.com/RustBlyat/ssnip
cd ssnip
cargo build --release
./target/release/ssnip
```

### Установка из исходников (Arch Linux)

В репозиторий включен `PKGBUILD` в стиле AUR:

```bash
git clone https://github.com/RustBlyat/ssnip
cd ssnip
makepkg -si
```

Бинарный файл устанавливается в `/usr/bin/ssnip`. Зависимости: `glibc`, `gcc-libs`, `wayland`, `libwebp`.

### Примечания и ограничения

- Требуется композитор Wayland с поддержкой протоколов `zwlr_layer_shell_v1` и `zwlr_screencopy_v1` (Hyprland, Sway, River, Wayfire, Labwc). Не поддерживается в X11 и GNOME/Mutter.
- Изолированный захват окон без перекрытий использует `grim` и сокеты управления окнами (Hyprland UNIX socket или Sway IPC). При их отсутствии автоматически используется прямоугольный вырез из общего кадра экрана.
- В буфер обмена изображение копируется как стандартный `image/png` для совместимости со всеми приложениями (Telegram, Discord, браузеры, графические редакторы).

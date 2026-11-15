# Проверка Windows UI и 3D-вида — 2026-11-15

Базовый коммит: `7bcca987b7aa65e3bc8d41e0cd746547c6684536`.
Версия исходников: 0.2.2. Хост: Windows 11 x86_64, Rust 1.99.0,
NVIDIA GeForce RTX 3050 Ti Laptop GPU, Vulkan.

## Автоматические проверки

- `cargo fmt --all --check`.
- `cargo clippy --locked -p open-bim-desktop --bin open-bim-desktop --test windows_interface -- -D warnings`.
- Desktop: `windows_interface` (7), `keyboard_only` (6), `project_change` (10),
  `unsaved_close` (7), `ribbon_chrome` (9), `ui_shell` (5).
- Renderer: `depth_shading` (4), `navigation_precision` (4), `viewport` (5).

57 выбранных тестов прошли. После последней правки размеров панели повторно
проверены Windows/клавиатурные сценарии, границы доступных элементов управления
и сворачивание групп ленты в узком окне. Все проверки прошли.
Локальная оптимизированная сборка использовала `CARGO_PROFILE_RELEASE_LTO=false`,
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`, `CARGO_INCREMENTAL=0`.

## Проверка установленного приложения

- Обычный запуск разворачивает окно; имя проекта отображается в системном заголовке.
- F1 показывает клавиши Windows; Ctrl+F переводит фокус в поиск модели.
- Ctrl+O и Ctrl+Shift+S открывают системные диалоги. Отмена не меняет модель и путь.
- Ctrl+A выбирает детали; Ctrl+C копирует 16 объектов; Ctrl+V запускает выбор точки
  вставки; Escape отменяет вставку без изменения модели.
- Снимки при 125% и 150%: вкладки не растут по высоте, панель свойств сохраняет
  левую границу, нижние элементы доступны прокруткой.
- Настоящий GPU-проход записал `VIEWPORT: MSAA 4x; native physical-pixel resolution`.
  Сравнены снимки одного узла до и после сглаживания/заполняющего освещения.
  Ошибок GPU не зарегистрировано.
- SHA-256 установленного EXE и EXE в переносном ZIP совпали.

Проверки macOS/Linux выполняются CI. Локальная проверка Windows не заменяет их.

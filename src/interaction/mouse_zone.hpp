#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>
#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

namespace booth {

/// A surface that accepts mouse hit-testing. `Link` is resolved from the
/// rendered OSC-8 hyperlink registry rather than from a zone.
enum class MouseSurface : unsigned char {
  None,
  Slash,
  File,
  Model,
  Session,
  History,
  Copy,
  Mention,
  Permission,
  Help,
  Transcript,
  Composer,
  Link,
};

/// Sentinel for "no hovered row".
inline constexpr std::size_t kNoMouseIndex = static_cast<std::size_t>(-1);

/// One rendered rectangle recorded during `SetBox`. Coordinates are final
/// screen cells, so callers never guess row heights or column origins.
struct MouseZone {
  MouseSurface surface{MouseSurface::None};
  std::size_t index{0};
  int x_min{0};
  int x_max{-1};
  int y_min{0};
  int y_max{-1};
  /// For `Composer`: the text origin relative to `x_min`.
  std::size_t column{0};
  /// For `Link` hits only.
  std::string url;
};

/// A resolved hit. `surface == MouseSurface::None` means empty space.
/// For `Composer`, `index` is the editor line and `column` is the zero-based
/// text column under the pointer (callers clamp it to the line length).
struct MouseHit {
  MouseSurface surface{MouseSurface::None};
  std::size_t index{0};
  std::size_t column{0};
  std::string url;

  [[nodiscard]] bool valid() const { return surface != MouseSurface::None; }
  explicit operator bool() const { return valid(); }
};

/// Classified mouse input. Wheel reports are returned but not consumed by the
/// router, so pickers and the transcript keep their current wheel behavior.
struct MouseInput {
  enum class Kind : unsigned char { None, Moved, Click, WheelUp, WheelDown };

  Kind kind{Kind::None};
  MouseHit hit;

  [[nodiscard]] bool consumed() const { return kind != Kind::None; }
};

/// Collects the rendered hit map for one frame and classifies mouse events
/// against it. Owned by the root component:
///   - call `begin_frame()` before building the view;
///   - pass `&router` to picker/composer views;
///   - call `handle(event, &screen)` from `OnEvent`.
class MouseRouter {
 public:
  /// Drops last frame's zones. Hover survives, so the highlight is stable
  /// across frames until the pointer moves off the row.
  void begin_frame();
  void clear();

  /// Wraps a row so its rendered box is recorded. `column` marks where text
  /// starts inside the row (used by the composer).
  ftxui::Element zone(ftxui::Element child, MouseSurface surface, std::size_t index,
                      std::size_t column = 0);

  /// Records a box. Called by the wrapper node from `SetBox`.
  void add(MouseZone zone);

  /// Hit-tests rendered zones; the last recorded containing zone wins.
  [[nodiscard]] std::optional<MouseHit> hit(int x, int y) const;

  /// Hit-tests a hyperlink cell first, then rendered zones. Pass the live
  /// `ScreenInteractive`/`Screen` from the root.
  [[nodiscard]] std::optional<MouseHit> hit(int x, int y,
                                            const ftxui::Screen& screen) const;

  /// Classifies one event. Motion only reports when the hover target changes;
  /// left presses always report `Click` (with an invalid hit for empty space).
  [[nodiscard]] MouseInput handle(ftxui::Event event,
                                  const ftxui::Screen* screen = nullptr);

  [[nodiscard]] bool hovered(MouseSurface surface, std::size_t index) const;
  [[nodiscard]] std::size_t hover_index(MouseSurface surface) const;
  [[nodiscard]] const std::optional<MouseHit>& hover() const { return hover_; }
  void clear_hover() { hover_.reset(); }

  [[nodiscard]] const std::vector<MouseZone>& zones() const { return zones_; }

 private:
  std::vector<MouseZone> zones_;
  std::optional<MouseHit> hover_;
};

/// The URL registered on the rendered cell, or empty when the cell has none.
std::string hyperlink_at(const ftxui::Screen& screen, int x, int y);

}  // namespace booth

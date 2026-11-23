#include "interaction/mouse_zone.hpp"

#include <utility>

namespace booth {
namespace {

bool contains(const MouseZone& zone, int x, int y) {
  return zone.x_max >= zone.x_min && zone.y_max >= zone.y_min && x >= zone.x_min &&
         x <= zone.x_max && y >= zone.y_min && y <= zone.y_max;
}

bool same_target(const std::optional<MouseHit>& left, const std::optional<MouseHit>& right) {
  if (!left.has_value() || !right.has_value()) return left.has_value() == right.has_value();
  return left->surface == right->surface && left->index == right->index &&
         left->url == right->url;
}

class MouseZoneNode : public ftxui::Node {
 public:
  MouseZoneNode(ftxui::Element child, MouseZone zone, MouseRouter* router)
      : child_(std::move(child)), zone_(std::move(zone)), router_(router) {}

  void ComputeRequirement() override {
    child_->ComputeRequirement();
    requirement_ = child_->requirement();
  }

  void SetBox(ftxui::Box box) override {
    child_->SetBox(box);
    if (router_ != nullptr) {
      zone_.x_min = box.x_min;
      zone_.x_max = box.x_max;
      zone_.y_min = box.y_min;
      zone_.y_max = box.y_max;
      router_->add(zone_);
    }
    ftxui::Node::SetBox(box);
  }

  void Select(ftxui::Selection& selection) override { child_->Select(selection); }

  void Check(ftxui::Node::Status* status) override { child_->Check(status); }

  std::string GetSelectedContent(ftxui::Selection& selection) override {
    return child_->GetSelectedContent(selection);
  }

  void Render(ftxui::Screen& screen) override { child_->Render(screen); }

 private:
  ftxui::Element child_;
  MouseZone zone_;
  MouseRouter* router_;
};

}  // namespace

void MouseRouter::begin_frame() {
  if (hover_.has_value()) {
    bool present = false;
    for (const auto& zone : zones_) {
      if (zone.surface == hover_->surface && zone.index == hover_->index &&
          zone.url == hover_->url) {
        present = true;
        break;
      }
    }
    if (!present) hover_.reset();
  }
  zones_.clear();
}

void MouseRouter::clear() {
  zones_.clear();
  hover_.reset();
}

ftxui::Element MouseRouter::zone(ftxui::Element child, MouseSurface surface, std::size_t index,
                                 std::size_t column) {
  MouseZone zone;
  zone.surface = surface;
  zone.index = index;
  zone.column = column;
  return std::make_shared<MouseZoneNode>(std::move(child), std::move(zone), this);
}

void MouseRouter::add(MouseZone zone) { zones_.push_back(std::move(zone)); }

std::optional<MouseHit> MouseRouter::hit(int x, int y) const {
  for (auto zone = zones_.rbegin(); zone != zones_.rend(); ++zone) {
    if (!contains(*zone, x, y)) continue;
    std::size_t column = 0;
    if (zone->surface == MouseSurface::Composer) {
      const int relative = x - zone->x_min - static_cast<int>(zone->column);
      column = relative > 0 ? static_cast<std::size_t>(relative) : 0;
    }
    return MouseHit{zone->surface, zone->index, column, zone->url};
  }
  return std::nullopt;
}

std::optional<MouseHit> MouseRouter::hit(int x, int y, const ftxui::Screen& screen) const {
  const std::string url = hyperlink_at(screen, x, y);
  if (!url.empty()) return MouseHit{MouseSurface::Link, 0, 0, url};
  return hit(x, y);
}

MouseInput MouseRouter::handle(ftxui::Event event, const ftxui::Screen* screen) {
  if (!event.is_mouse()) return {};
  const auto& mouse = event.mouse();
  if (mouse.button == ftxui::Mouse::WheelUp) return {MouseInput::Kind::WheelUp, {}};
  if (mouse.button == ftxui::Mouse::WheelDown) return {MouseInput::Kind::WheelDown, {}};

  const auto resolve = [&]() -> std::optional<MouseHit> {
    return screen == nullptr ? hit(mouse.x, mouse.y) : hit(mouse.x, mouse.y, *screen);
  };

  if (mouse.motion == ftxui::Mouse::Moved) {
    auto target = resolve();
    if (same_target(hover_, target)) return {};
    hover_ = target;
    return {MouseInput::Kind::Moved, target.value_or(MouseHit{})};
  }

  if (mouse.button == ftxui::Mouse::Left && mouse.motion == ftxui::Mouse::Pressed) {
    auto target = resolve();
    hover_ = target;
    return {MouseInput::Kind::Click, target.value_or(MouseHit{})};
  }

  return {};
}

bool MouseRouter::hovered(MouseSurface surface, std::size_t index) const {
  return hover_.has_value() && hover_->surface == surface && hover_->index == index;
}

std::size_t MouseRouter::hover_index(MouseSurface surface) const {
  if (!hover_.has_value() || hover_->surface != surface) return kNoMouseIndex;
  return hover_->index;
}

std::string hyperlink_at(const ftxui::Screen& screen, int x, int y) {
  if (x < 0 || y < 0 || x >= screen.dimx() || y >= screen.dimy()) return {};
  const auto& cell = screen.CellAt(x, y);
  if (cell.hyperlink == 0) return {};
  return screen.Hyperlink(cell.hyperlink);
}

}  // namespace booth

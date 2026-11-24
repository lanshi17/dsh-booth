#include "interaction/view.hpp"

#include "interaction/theme.hpp"
#include "interaction/tool_diff.hpp"

#include <ftxui/dom/elements.hpp>
#include <nlohmann/json.hpp>

#include <algorithm>
#include <string>

namespace booth {
namespace {

#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kGreen (theme_color("green", ftxui::Color::RGB(0, 255, 135)))
#define kRed (theme_color("red", ftxui::Color::RGB(255, 95, 95)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kLine (theme_color("line", ftxui::Color::RGB(72, 72, 72)))
#define kPanel (theme_color("panel", ftxui::Color::RGB(18, 18, 18)))

std::string hrule(std::size_t count) {
  std::string out;
  for (std::size_t index = 0; index < count; ++index) out += "─";
  return out;
}

}  // namespace

ftxui::Element permission_view(const PermissionRequest& request, MouseRouter* mouse) {
  ftxui::Elements body;
  body.push_back(ftxui::text("permission: " + request.tool) | ftxui::bold | ftxui::color(kAccent));
  if (!request.detail.empty()) {
    TranscriptEntry entry;
    entry.tool = request.tool;
    entry.raw_input = request.detail;
    const auto diff = tool_diff(entry);
    if (!diff.empty()) {
      std::size_t shown = 0;
      std::size_t hidden = 0;
      for (const auto& line : diff) {
        if (shown >= 8) {
          ++hidden;
          continue;
        }
        ++shown;
        const ftxui::Color color = line.change < 0 ? kRed : line.change > 0 ? kGreen : kDim;
        body.push_back(ftxui::text(line.text) | ftxui::color(color));
      }
      if (hidden > 0) {
        body.push_back(ftxui::text("… +" + std::to_string(hidden) + " lines") | ftxui::dim);
      }
    } else {
      const auto parsed = nlohmann::json::parse(request.detail, nullptr, false);
      if (!parsed.is_discarded() && parsed.contains("command") && parsed["command"].is_string()) {
        const std::string command = parsed["command"].get<std::string>();
        body.push_back(ftxui::paragraph(request.tool == "bash" ? "$ " + command : command));
      } else {
        body.push_back(ftxui::paragraph(request.detail));
      }
    }
  }
  body.push_back(ftxui::text(hrule(58)) | ftxui::color(kLine));
  auto button = [&](std::string label, ftxui::Color color, std::size_t index) {
    auto element = ftxui::text(std::move(label)) | ftxui::color(color);
    return mouse == nullptr ? element
                            : mouse->zone(std::move(element), MouseSurface::Permission, index);
  };
  body.push_back(ftxui::hbox({button("[y] Allow once", kGreen, 0),
                              ftxui::text("    "),
                              button("[a] Allow always", kGreen, 1)}));
  body.push_back(ftxui::hbox({button("[n] Deny", kRed, 2),
                              ftxui::text("          "),
                              button("[d] Deny always", kRed, 3),
                              ftxui::text("    (Esc)") | ftxui::dim}));
  return ftxui::hbox({ftxui::text(" "), ftxui::vbox(std::move(body)), ftxui::filler()}) |
         ftxui::bgcolor(kPanel);
}

}  // namespace booth

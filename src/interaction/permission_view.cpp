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
  std::string heading = "permission: " + request.tool;
  if (!request.title.empty() && request.title != request.tool) heading += " · " + request.title;
  body.push_back(ftxui::text(heading) | ftxui::bold | ftxui::color(kAccent));
  const std::string& detail = request.raw_input.empty() ? request.detail : request.raw_input;
  if (!detail.empty()) {
    TranscriptEntry entry;
    entry.tool = request.tool;
    entry.raw_input = detail;
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
      const auto parsed = nlohmann::json::parse(detail, nullptr, false);
      if (!parsed.is_discarded() && parsed.contains("command") && parsed["command"].is_string()) {
        const std::string command = parsed["command"].get<std::string>();
        body.push_back(ftxui::paragraph(request.tool == "bash" ? "$ " + command : command));
      } else {
        body.push_back(ftxui::paragraph(detail));
      }
    }
  }
  body.push_back(ftxui::text(hrule(58)) | ftxui::color(kLine));
  auto button = [&](std::string label, ftxui::Color color, std::size_t index) {
    auto element = ftxui::text(std::move(label)) | ftxui::color(color);
    return mouse == nullptr ? element
                            : mouse->zone(std::move(element), MouseSurface::Permission, index);
  };
  const auto key_hint = [](const std::string& kind) -> std::string {
    if (kind == "allow_once") return "y";
    if (kind == "allow_always") return "a";
    if (kind == "reject_once") return "n";
    if (kind == "reject_always") return "d";
    return "?";
  };
  ftxui::Elements row;
  for (std::size_t index = 0; index < request.options.size(); ++index) {
    const auto& option = request.options[index];
    const bool allow = option.kind.rfind("allow", 0) == 0;
    std::string label = "[" + key_hint(option.kind) + "] " +
                        (option.name.empty() ? option.kind : option.name);
    if (!row.empty()) row.push_back(ftxui::text("    "));
    row.push_back(button(std::move(label), allow ? kGreen : kRed, index));
    if (row.size() == 3) {
      body.push_back(ftxui::hbox(std::move(row)));
      row.clear();
    }
  }
  if (!row.empty()) body.push_back(ftxui::hbox(std::move(row)));
  if (request.options.empty()) {
    body.push_back(ftxui::text("no options · Esc cancels") | ftxui::dim);
  } else {
    body.push_back(ftxui::text("(Esc)") | ftxui::dim);
  }
  return ftxui::hbox({ftxui::text(" "), ftxui::vbox(std::move(body)), ftxui::filler()}) |
         ftxui::bgcolor(kPanel);
}

}  // namespace booth

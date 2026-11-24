#include "interaction/thought_view.hpp"

#include "interaction/theme.hpp"
#include "interaction/view.hpp"

#include <chrono>
#include <string>

namespace booth {

std::string thought_label(const TranscriptEntry& entry) {
  const auto zero = std::chrono::system_clock::time_point{};
  if (entry.started == zero) return "Thought";
  const bool live = entry.finished == zero;
  const auto end = live ? std::chrono::system_clock::now() : entry.finished;
  const auto milliseconds =
      std::chrono::duration_cast<std::chrono::milliseconds>(end - entry.started).count();
  long long seconds = milliseconds / 1000;
  if (seconds < 1) seconds = 1;
  std::string elapsed;
  if (seconds < 60) {
    elapsed = std::to_string(seconds) + "s";
  } else if (seconds < 3600) {
    elapsed = std::to_string(seconds / 60) + "m" + std::to_string(seconds % 60) + "s";
  } else {
    elapsed = std::to_string(seconds / 3600) + "h" + std::to_string((seconds % 3600) / 60) + "m";
  }
  return (live ? "Thinking… " : "Thought for ") + elapsed;
}

ftxui::Element thought_view(const TranscriptEntry& entry) {
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const bool live = entry.finished == std::chrono::system_clock::time_point{};
  ftxui::Elements head;
  head.push_back(ftxui::text("  ") | ftxui::color(dim));
  if (live) head.push_back(ftxui::text("✻ ") | ftxui::color(accent));
  head.push_back(ftxui::text(thought_label(entry)) | ftxui::color(dim));
  return ftxui::vbox({ftxui::hbox(std::move(head)), markdown_view(entry.text, true),
                      ftxui::text(" ")});
}

}  // namespace booth

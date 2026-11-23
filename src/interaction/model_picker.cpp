#include "interaction/model_picker.hpp"

#include <algorithm>
#include <cctype>

namespace booth {
namespace {

std::string lower(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

void erase_last_glyph(std::string& text) {
  if (text.empty()) return;
  std::size_t at = text.size() - 1;
  while (at > 0 && (static_cast<unsigned char>(text[at]) & 0xC0) == 0x80) --at;
  text.erase(at);
}

}  // namespace

void ModelPicker::open(const std::vector<ModelChoice>& choices, const std::string& current) {
  visible_ = true;
  query_.clear();
  index_ = 0;
  filter(choices);
  for (std::size_t row = 0; row < matches_.size(); ++row) {
    if (choices[matches_[row]].name == current) {
      index_ = row;
      break;
    }
  }
}

void ModelPicker::close() { visible_ = false; }

void ModelPicker::filter(const std::vector<ModelChoice>& choices) {
  matches_.clear();
  const std::string needle = lower(query_);
  for (std::size_t index = 0; index < choices.size(); ++index) {
    const auto& choice = choices[index];
    if (needle.empty() ||
        lower(choice.name).find(needle) != std::string::npos ||
        lower(choice.provider).find(needle) != std::string::npos ||
        lower(choice.value).find(needle) != std::string::npos) {
      matches_.push_back(index);
    }
  }
  if (index_ >= matches_.size()) index_ = 0;
}

std::optional<ModelChoice> ModelPicker::click(std::size_t row,
                                              const std::vector<ModelChoice>& choices) {
  if (!visible_ || row >= matches_.size()) return std::nullopt;
  index_ = row;
  const auto chosen = choices[matches_[index_]];
  close();
  return chosen;
}

std::optional<ModelChoice> ModelPicker::key(ftxui::Event& event,
                                            const std::vector<ModelChoice>& choices) {
  if (!visible_) return std::nullopt;
  if (event == ftxui::Event::Escape) {
    close();
    return std::nullopt;
  }
  if (event == ftxui::Event::Backspace) {
    erase_last_glyph(query_);
    filter(choices);
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowUp ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp)) {
    if (!matches_.empty()) index_ = (index_ + matches_.size() - 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowDown ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown)) {
    if (!matches_.empty()) index_ = (index_ + 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::Return) {
    if (matches_.empty()) {
      close();
      return std::nullopt;
    }
    const auto chosen = choices[matches_[index_]];
    close();
    return chosen;
  }
  if (event.is_character()) {
    query_ += event.input();
    filter(choices);
  }
  return std::nullopt;
}

}  // namespace booth

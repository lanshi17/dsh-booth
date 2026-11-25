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

void ModelPicker::open(const std::vector<ModelChoice>& choices, const std::string& current,
                       const std::string& provider) {
  visible_ = true;
  query_.clear();
  provider_ = provider;
  providers_.clear();
  for (const auto& choice : choices) providers_.push_back(choice.provider);
  std::sort(providers_.begin(), providers_.end());
  providers_.erase(std::unique(providers_.begin(), providers_.end()), providers_.end());
  provider_index_ = 0;
  if (!provider_.empty()) {
    for (std::size_t at = 0; at < providers_.size(); ++at) {
      if (providers_[at] == provider_) { provider_index_ = at + 1; break; }
    }
  }
  model_focus_ = false;
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
    if (!provider_.empty() && choice.provider != provider_) continue;
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
  const bool wheel_up = event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp;
  const bool wheel_down = event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown;
  if ((event == ftxui::Event::ArrowUp || wheel_up) && !model_focus_) {
    const std::size_t count = providers_.size() + 1;
    provider_index_ = (provider_index_ + count - 1) % count;
    provider_ = provider_index_ == 0 ? std::string{} : providers_[provider_index_ - 1];
    index_ = 0;
    filter(choices);
    return std::nullopt;
  }
  if ((event == ftxui::Event::ArrowDown || wheel_down) && !model_focus_) {
    const std::size_t count = providers_.size() + 1;
    provider_index_ = (provider_index_ + 1) % count;
    provider_ = provider_index_ == 0 ? std::string{} : providers_[provider_index_ - 1];
    index_ = 0;
    filter(choices);
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowUp || wheel_up) {
    if (!matches_.empty()) index_ = (index_ + matches_.size() - 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowDown || wheel_down) {
    if (!matches_.empty()) index_ = (index_ + 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowRight ||
      (event == ftxui::Event::Return && !model_focus_)) {
    model_focus_ = true;
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowLeft) {
    model_focus_ = false;
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
    model_focus_ = true;
    query_ += event.input();
    filter(choices);
  }
  return std::nullopt;
}

}  // namespace booth

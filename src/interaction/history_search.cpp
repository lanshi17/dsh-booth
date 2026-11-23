#include "interaction/history_search.hpp"

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

void HistorySearch::open(const std::vector<std::string>& history) {
  all_.clear();
  all_.reserve(history.size());
  for (auto entry = history.rbegin(); entry != history.rend(); ++entry) {
    if (std::find(all_.begin(), all_.end(), *entry) != all_.end()) continue;
    all_.push_back(*entry);
  }
  visible_ = true;
  query_.clear();
  index_ = 0;
  filter();
}

void HistorySearch::close() { visible_ = false; }

void HistorySearch::filter() {
  matches_.clear();
  const std::string needle = lower(query_);
  for (const auto& entry : all_) {
    if (needle.empty() || lower(entry).find(needle) != std::string::npos) {
      matches_.push_back(entry);
    }
  }
  if (index_ >= matches_.size()) index_ = 0;
}

HistorySearch::Result HistorySearch::handle(ftxui::Event& event, AppState& state,
                                             const std::vector<std::string>& history,
                                             bool open_key) {
  if (!visible_ && open_key) {
    open(history);
    return Result::Handled;
  }
  if (!visible_) return Result::None;
  return key(event, state) ? Result::Accepted : Result::Handled;
}

bool HistorySearch::click(std::size_t row, AppState& state) {
  if (!visible_ || row >= matches_.size()) return false;
  index_ = row;
  state.composer = matches_[index_];
  close();
  return true;
}

bool HistorySearch::key(ftxui::Event& event, AppState& state) {
  if (!visible_) return false;
  if (event == ftxui::Event::Escape) {
    close();
    return false;
  }
  if (event == ftxui::Event::Backspace) {
    erase_last_glyph(query_);
    filter();
    return true;
  }
  if (event == ftxui::Event::ArrowUp) {
    if (!matches_.empty()) index_ = (index_ + matches_.size() - 1) % matches_.size();
    return true;
  }
  if (event == ftxui::Event::ArrowDown) {
    if (!matches_.empty()) index_ = (index_ + 1) % matches_.size();
    return true;
  }
  if (event == ftxui::Event::Return) {
    const bool accepted = !matches_.empty();
    if (accepted) state.composer = matches_[index_];
    close();
    return accepted;
  }
  if (event.is_character()) {
    query_ += event.input();
    filter();
    return true;
  }
  return true;
}

}  // namespace booth

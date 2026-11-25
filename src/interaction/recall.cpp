#include "interaction/recall.hpp"

#include <algorithm>
#include <utility>

namespace booth {

void PromptRecall::clear(std::string draft) {
  if (draft.empty()) return;
  cleared_.push_front(std::move(draft));
  if (cleared_.size() > kLimit) cleared_.pop_back();
  position_ = 0;
}

void PromptRecall::reset() { position_ = 0; }

std::string PromptRecall::at(std::size_t position, const std::vector<std::string>& history) const {
  if (position == 0) return {};
  const std::size_t index = position - 1;
  if (index < cleared_.size()) return cleared_[index];
  const std::size_t history_index = history.size() - (index - cleared_.size()) - 1;
  return history_index < history.size() ? history[history_index] : std::string{};
}

std::optional<std::string> PromptRecall::up(const std::vector<std::string>& history) {
  const std::size_t total = cleared_.size() + history.size();
  if (position_ >= total) return std::nullopt;
  ++position_;
  return at(position_, history);
}

std::optional<std::string> PromptRecall::down(const std::vector<std::string>& history) {
  if (position_ == 0) return std::nullopt;
  --position_;
  return at(position_, history);
}

}  // namespace booth

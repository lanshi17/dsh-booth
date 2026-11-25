#include "interaction/vim.hpp"

#include <algorithm>
#include <string>

namespace booth::vim {

void VimState::Reset() {
  mode = Mode::Normal;
  anchor.reset();
  desired_col_.reset();
  ClearPending();
  last_find_.reset();
  cmd_keys_.clear();
  cmd_count_.clear();
  last_change_.reset();
  staged_change_.reset();
}

bool VimState::IsCountKey(std::string_view key) const {
  return !find_forward_ && !replace_char_ &&
         ((key.size() == 1 && key[0] >= '1' && key[0] <= '9') || (key == "0" && !count_.empty()));
}

void VimState::ClearPending() {
  count_.clear();
  operator_ = Operator::None;
  register_pending_ = false;
  register_ = 0;
  pending_g_ = false;
  text_object_.clear();
  find_forward_.reset();
  find_till_.reset();
  replace_char_ = false;
}

int VimState::TakeCount() {
  const int count = count_.empty() ? 1 : std::max(1, std::stoi(count_));
  count_.clear();
  return count;
}

}  // namespace booth::vim

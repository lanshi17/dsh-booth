#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include "interaction/scroll.hpp"
#include "interaction/state.hpp"

namespace booth {

/// `/find`: match transcript entries and scroll the newest match into view.
class TranscriptSearch {
 public:
  std::string find(AppState& state, TranscriptScroll& scroll, const std::string& argument);

 private:
  void collect(const AppState& state);

  std::string query_;
  std::vector<std::size_t> matches_;
  std::size_t position_{0};
  bool has_query_{false};
};

}  // namespace booth

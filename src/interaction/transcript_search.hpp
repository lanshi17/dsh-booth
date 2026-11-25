#pragma once

#include <cstddef>
#include <functional>
#include <string>
#include <vector>

#include "interaction/scroll.hpp"
#include "interaction/state.hpp"

namespace booth {

/// Which entries the view is currently drawing; `/find` never matches a
/// hidden card, because focusing it would scroll to an empty row.
using VisibleEntry = std::function<bool(const TranscriptEntry&)>;

/// `/find`: match transcript entries and scroll the newest match into view.
class TranscriptSearch {
 public:
  std::string find(AppState& state, TranscriptScroll& scroll, const std::string& argument,
                   const VisibleEntry& visible = {});

 private:
  void collect(const AppState& state, const VisibleEntry& visible);

  std::string query_;
  std::vector<std::size_t> matches_;
  std::size_t position_{0};
  bool has_query_{false};
};

}  // namespace booth

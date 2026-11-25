#pragma once

#include <optional>
#include <string>
#include <vector>

#include "interaction/state.hpp"

namespace booth {

/// `command` is canonical (`/effort` for `/reasoning`); `prefix` is the argument text after it.
struct ArgumentQuery {
  std::string command;
  std::string prefix;
};

struct ArgumentHint {
  std::string value;
  std::string description;
};

/// Parses a canonical command plus its argument prefix, or nullopt outside a known argument command.
std::optional<ArgumentQuery> argument_query(const std::string& composer);

/// Values that complete this command's argument prefix, best order preserved.
std::vector<ArgumentHint> matching_arguments(const ArgumentQuery& query, const AppState& state);

}  // namespace booth

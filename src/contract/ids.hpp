#pragma once

#include <compare>
#include <string>

namespace booth {

struct SessionId {
  std::string value;
  auto operator<=>(const SessionId&) const = default;
};


struct RequestId {
  std::string value;
  auto operator<=>(const RequestId&) const = default;
};

struct TurnId {
  std::string value;
  auto operator<=>(const TurnId&) const = default;
};

}  // namespace booth

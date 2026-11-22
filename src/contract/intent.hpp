#pragma once

#include <string>
#include <variant>

#include "contract/ids.hpp"

namespace booth {

struct Prompt {
  SessionId session;
  std::string text;
};

struct Cancel {
  SessionId session;
};

struct AnswerPermission {
  RequestId request;
  std::string option;  // option id from the request; empty means cancelled
};

struct NewSession {
  std::string cwd;
};

struct Refresh {};

struct EnsureDsh {};

struct Quit {};

using IntentValue =
    std::variant<Prompt, Cancel, AnswerPermission, NewSession, Refresh, EnsureDsh, Quit>;

struct Intent {
  IntentValue value;
};

}  // namespace booth

#pragma once

#include <string>
#include <vector>

namespace booth {

struct ModelChoice {
  std::string provider;
  std::string name;
  std::string value;
};

struct ReasoningOptions {
  std::string id;
  std::string current;
  std::string current_name;
  std::vector<ModelChoice> choices;
};

}  // namespace booth

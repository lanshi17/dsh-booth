#pragma once

#include <string>
#include <string_view>
#include <vector>

#include "control/client.hpp"
#include "contract/error.hpp"

namespace booth::provider {

struct Field {
  std::string label;
  std::string value;
  std::string hint;
  bool secret{false};
  bool read_only{false};
};

/// One route in the dsh provider directory.
struct Row {
  std::string name;
  std::string label;
  std::string detail;
  bool configured{false};
};

struct Listing {
  std::vector<Row> rows;
  std::string error;
};

/// Load the configurable provider directory through the booth control socket.
Listing list(ControlClient& control);

/// Form state for one route. Field order is fixed: name, displayName, api,
/// baseURL, models, apiKey.
struct Form {
  bool create{false};
  bool configured{false};
  std::string provider;
  std::vector<Field> fields;
  std::vector<std::string> initial;
  std::size_t active{0};
  std::string status;
  bool status_error{false};
};

/// Load `provider`, or start a blank route when `create` is true.
Result<Form> load(ControlClient& control, std::string provider, bool create);

/// Save the form through `providers.update`. Returns false and fills the form
/// status on failure. On success `notice` carries dsh's text.
bool save(ControlClient& control, Form& form, std::string& notice);

/// Remove the route's user layer and key. Returns false on failure.
bool remove(ControlClient& control, Form& form, std::string& notice);

/// Ask dsh to discover endpoint models and fill the models field.
bool discover(ControlClient& control, Form& form);

std::vector<std::string> split_models(std::string_view text);

}  // namespace booth::provider

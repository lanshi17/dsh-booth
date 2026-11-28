#include "provider/model.hpp"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <thread>

#include <nlohmann/json.hpp>

namespace booth::provider {
namespace {

using json = nlohmann::json;

std::string trim(std::string_view text) {
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.front())) != 0) text.remove_prefix(1);
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.back())) != 0) text.remove_suffix(1);
  return std::string(text);
}

std::string join_models(const json& models) {
  std::string out;
  if (!models.is_array()) return out;
  for (const auto& model : models) {
    const std::string id = model.is_object() ? model.value("id", std::string{})
                                             : model.is_string() ? model.get<std::string>() : std::string{};
    if (id.empty()) continue;
    if (!out.empty()) out += ", ";
    out += id;
  }
  return out;
}

json request_with_retry(ControlClient& control, std::string_view method, const json& params,
                        std::chrono::milliseconds timeout) {
  auto result = control.request(method, params, timeout);
  if (result) return *result;
  std::this_thread::sleep_for(std::chrono::milliseconds(300));
  result = control.request(method, params, timeout);
  if (result) return *result;
  return json{{"__error", result.error().detail.empty() ? to_string(result.error())
                                                          : result.error().detail}};
}

std::string error_of(const json& value) {
  return value.value("__error", std::string{});
}

}  // namespace

Listing list(ControlClient& control) {
  Listing listing;
  for (int attempt = 0; attempt < 3; ++attempt) {
    if (attempt > 0) std::this_thread::sleep_for(std::chrono::milliseconds(300));
    auto result = control.request("providers.list", json::object(), std::chrono::seconds(10));
    if (!result) {
      listing.error = to_string(result.error());
      continue;
    }
    listing.error.clear();
    const auto providers = result->value("providers", json::array());
    if (!providers.is_array()) return listing;
    for (const auto& provider : providers) {
      if (!provider.is_object()) continue;
      Row row;
      row.name = provider.value("provider", std::string{});
      if (row.name.empty()) continue;
      row.configured = provider.value("configured", true);
      row.has_key = provider.value("apiKeyConfigured", false);
      row.label = provider.value("displayName", row.name);
      const std::string base = provider.value("baseURL", std::string{});
      const std::string fallback_base = provider.value("defaultBaseURL", std::string{});
      const bool key = provider.value("apiKeyConfigured", false);
      const std::string masked = provider.value("apiKeyMasked", std::string{});
      if (!row.configured) {
        row.detail = "not configured";
        if (!fallback_base.empty()) row.detail += " · default " + fallback_base;
      } else {
        row.detail = base.empty() ? "configured" : base;
        row.detail += key ? " · key " + masked : " · no key";
        if (!provider.value("removable", true)) row.detail += " · base";
      }
      const std::string error = provider.value("error", std::string{});
      if (!error.empty()) row.detail += " · error: " + error;
      listing.rows.push_back(std::move(row));
    }
    return listing;
  }
  return listing;
}

Result<Form> load(ControlClient& control, std::string provider, bool create) {
  Form form;
  form.create = create;
  form.provider = trim(provider);
  json current = json::object();
  if (!form.provider.empty()) {
    current = request_with_retry(control, "providers.get", {{"provider", form.provider}},
                                 std::chrono::seconds(10));
    const std::string error = error_of(current);
    if (!error.empty()) {
      if (!create) return std::unexpected(Error{Layer::Kernel, error, std::nullopt});
      current = json::object();
    }
  }
  form.configured = current.value("configured", false);
  if (create && form.configured) {
    return std::unexpected(Error{Layer::Kernel, form.provider + " is already configured", std::nullopt});
  }
  const std::string display = current.value("displayName", form.provider);
  const std::string api = current.value("api", std::string{});
  const std::string base = current.value("baseURL", std::string{});
  const std::string models = join_models(current.value("models", json::array()));
  const bool key = current.value("apiKeyConfigured", false);
  const std::string masked = current.value("apiKeyMasked", std::string{});

  form.fields.push_back({"name", form.provider, "route name", false, !create});
  form.fields.push_back({"displayName", display == form.provider ? std::string{} : display,
                         "label; blank uses the route name", false, false});
  form.fields.push_back({"api", api, "e.g. openai-completions; blank uses the catalog default", false, false});
  form.fields.push_back({"baseURL", base, "blank restores the provider default", false, false});
  form.fields.push_back({"models", models, "comma-separated ids; blank uses catalog models", false, false});
  form.fields.push_back({"apiKey", "",
                         key ? (masked.empty() ? "current key configured" : "current key " + masked) +
                                   "; blank keeps it; !clear removes it"
                             : "blank leaves the route keyless; !clear removes a stored key",
                         true, false});
  form.initial.reserve(form.fields.size());
  for (const auto& field : form.fields) form.initial.push_back(field.value);
  while (form.active < form.fields.size() && form.fields[form.active].read_only) ++form.active;
  if (!form.fields.empty()) form.cursor = form.fields[form.active].value.size();
  form.status = create ? "new provider · name it, fill the endpoint, then Ctrl+S"
                       : (form.configured ? "editing user configuration"
                                          : "this catalog route is not configured yet");
  return form;
}

bool save(ControlClient& control, Form& form, std::string& notice) {
  if (form.fields.empty()) return false;
  const std::string provider = trim(form.fields[0].value);
  if (provider.empty()) {
    form.status = "provider name is required";
    form.status_error = true;
    return false;
  }
  json params = {{"provider", provider}};
  const auto changed = [&](std::size_t index) {
    return index < form.fields.size() && index < form.initial.size() &&
           form.fields[index].value != form.initial[index];
  };
  const auto include = [&](std::size_t index, const char* key, bool create_nonempty) {
    if (index >= form.fields.size()) return;
    if (form.create ? (create_nonempty && !form.fields[index].value.empty()) : changed(index)) {
      params[key] = form.fields[index].value;
    }
  };
  include(1, "displayName", true);
  include(2, "api", true);
  include(3, "baseURL", true);
  if (4 < form.fields.size()) {
    std::vector<std::string> models = split_models(form.fields[4].value);
    if (form.create ? !models.empty() : form.fields[4].value != form.initial[4]) {
      params["models"] = models;
    }
  }
  if (5 < form.fields.size() && !form.fields[5].value.empty()) {
    if (form.fields[5].value == "!clear") params["removeKey"] = true;
    else params["apiKey"] = form.fields[5].value;
  }
  if (params.size() == 1) {
    form.status = "no changes";
    form.status_error = false;
    return false;
  }
  const json result = request_with_retry(control, "providers.update", params, std::chrono::seconds(30));
  const std::string error = error_of(result);
  if (!error.empty()) {
    form.status = error;
    form.status_error = true;
    return false;
  }
  notice = result.value("text", std::string{"provider updated"});
  form.status = notice;
  form.status_error = false;
  return true;
}

bool remove(ControlClient& control, Form& form, std::string& notice) {
  if (form.create || !form.configured) {
    form.status = "this route has no user configuration to remove";
    form.status_error = true;
    return false;
  }
  const json result = request_with_retry(control, "providers.remove",
                                         {{"provider", form.provider}, {"removeKey", true}},
                                         std::chrono::seconds(30));
  const std::string error = error_of(result);
  if (!error.empty()) {
    form.status = error;
    form.status_error = true;
    return false;
  }
  notice = result.value("text", std::string{"provider removed"});
  form.status = notice;
  form.status_error = false;
  return true;
}

bool discover(ControlClient& control, Form& form) {
  if (form.fields.empty()) return false;
  const std::string provider = trim(form.fields[0].value);
  if (provider.empty()) {
    form.status = "set the provider name before discovering models";
    form.status_error = true;
    return false;
  }
  json params = {{"provider", provider}};
  if (!form.fields[3].value.empty()) params["baseURL"] = form.fields[3].value;
  if (!form.fields[2].value.empty()) params["api"] = form.fields[2].value;
  const json result = request_with_retry(control, "providers.discover", params, std::chrono::seconds(30));
  const std::string error = error_of(result);
  if (!error.empty()) {
    form.status = error;
    form.status_error = true;
    return false;
  }
  const auto models = result.value("models", json::array());
  std::vector<std::string> ids;
  if (models.is_array()) {
    for (const auto& model : models) {
      if (!model.is_object()) continue;
      const std::string id = model.value("id", std::string{});
      if (!id.empty()) ids.push_back(id);
    }
  }
  std::string joined;
  for (std::size_t index = 0; index < ids.size() && index < 30; ++index) {
    if (!joined.empty()) joined += ", ";
    joined += ids[index];
  }
  if (!joined.empty()) form.fields[4].value = joined;
  form.status = ids.empty() ? "provider advertised no models"
                            : "discovered " + std::to_string(ids.size()) + " models";
  form.status_error = ids.empty();
  return !ids.empty();
}

std::vector<std::string> split_models(std::string_view text) {
  std::vector<std::string> out;
  std::string current;
  for (const char ch : text) {
    if (ch == ',' || ch == '\n' || ch == '\r') {
      const std::string id = trim(current);
      if (!id.empty()) out.push_back(id);
      current.clear();
    } else {
      current += ch;
    }
  }
  const std::string id = trim(current);
  if (!id.empty()) out.push_back(id);
  return out;
}

}  // namespace booth::provider

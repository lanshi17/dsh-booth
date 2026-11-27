#include "interaction/provider_page.hpp"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <sstream>
#include <string>
#include <string_view>
#include <thread>
#include <vector>

#include <nlohmann/json.hpp>

#include "interaction/theme.hpp"

namespace booth {
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

void erase_glyph(std::string& value) {
  if (value.empty()) return;
  std::size_t at = value.size() - 1;
  while (at > 0 && (static_cast<unsigned char>(value[at]) & 0xC0) == 0x80) --at;
  value.erase(at);
}

json request_with_retry(ControlClient& control, std::string_view method, const json& params,
                        std::chrono::milliseconds timeout) {
  auto result = control.request(method, params, timeout);
  if (result) return *result;
  std::this_thread::sleep_for(std::chrono::milliseconds(300));
  result = control.request(method, params, timeout);
  if (result) return *result;
  return json{{"__error", result.error().detail}};
}

}  // namespace

void ProviderPage::open(ControlClient& control, AppState& state, std::string provider, bool create) {
  provider_ = trim(provider);
  create_ = create;
  status_.clear();
  status_error_ = false;
  fields_.clear();
  initial_.clear();
  active_ = 0;
  // A blank name is the first field of a new route; load nothing.

  json current = json::object();
  if (!provider_.empty()) {
    current = request_with_retry(control, "providers.get", {{"provider", provider_}},
                                 std::chrono::seconds(10));
    if (current.contains("__error")) {
      if (!create_) {
        state.notice = current["__error"].get<std::string>();
        return;
      }
      current = json::object();
    }
  }
  configured_ = current.value("configured", false);
  if (create_ && configured_) {
    state.notice = provider_ + " is already configured";
    return;
  }
  const std::string display = current.value("displayName", provider_);
  const std::string api = current.value("api", std::string{});
  const std::string base = current.value("baseURL", std::string{});
  const std::string models = join_models(current.value("models", json::array()));
  const bool key = current.value("apiKeyConfigured", false);
  const std::string masked = current.value("apiKeyMasked", std::string{});

  fields_.push_back({"name", provider_, "route name", false, !create_});
  fields_.push_back({"displayName", display == provider_ ? std::string{} : display, "label; blank uses the route name", false, false});
  fields_.push_back({"api", api, "e.g. openai-completions; blank uses the catalog default", false, false});
  fields_.push_back({"baseURL", base, "blank restores the provider default", false, false});
  fields_.push_back({"models", models, "comma-separated ids; blank uses catalog models", false, false});
  fields_.push_back({"apiKey", "", key ? (masked.empty() ? "current key configured" : "current key " + masked) +
                                             "; blank keeps it; !clear removes it"
                                       : "blank leaves the route keyless; !clear removes a stored key",
                     true, false});
  initial_.resize(fields_.size());
  for (std::size_t index = 0; index < fields_.size(); ++index) initial_[index] = fields_[index].value;
  while (active_ < fields_.size() && fields_[active_].read_only) ++active_;
  visible_ = true;
  status_ = create_ ? "new provider · name it, fill the endpoint, then Ctrl+S"
                    : (configured_ ? "editing user configuration" : "this catalog route is not configured yet");
}

void ProviderPage::close() { visible_ = false; }

std::size_t ProviderPage::editable_count() const {
  return static_cast<std::size_t>(std::count_if(fields_.begin(), fields_.end(),
                                                [](const Field& field) { return !field.read_only; }));
}

bool ProviderPage::last_editable() const {
  for (std::size_t index = fields_.size(); index > 0; --index) {
    if (!fields_[index - 1].read_only) return active_ == index - 1;
  }
  return true;
}

void ProviderPage::move(int step) {
  if (fields_.empty()) return;
  int index = static_cast<int>(active_);
  for (std::size_t guard = 0; guard < fields_.size(); ++guard) {
    index = (index + step + static_cast<int>(fields_.size())) % static_cast<int>(fields_.size());
    if (!fields_[static_cast<std::size_t>(index)].read_only) {
      active_ = static_cast<std::size_t>(index);
      return;
    }
  }
}

void ProviderPage::erase_last() {
  if (fields_.empty() || fields_[active_].read_only) return;
  erase_glyph(fields_[active_].value);
}

void ProviderPage::discover(ControlClient& control, AppState& state) {
  if (fields_.empty()) return;
  const std::string provider = trim(fields_[0].value);
  if (provider.empty()) {
    status_ = "set the provider name before discovering models";
    status_error_ = true;
    return;
  }
  json params = {{"provider", provider}};
  if (!fields_[3].value.empty()) params["baseURL"] = fields_[3].value;
  if (!fields_[2].value.empty()) params["api"] = fields_[2].value;
  const json result = request_with_retry(control, "providers.discover", params, std::chrono::seconds(30));
  if (result.contains("__error")) {
    status_ = result["__error"].get<std::string>();
    status_error_ = true;
    return;
  }
  const auto models = result.value("models", json::array());
  std::vector<std::string> ids;
  for (const auto& model : models) {
    if (model.is_object()) {
      const std::string id = model.value("id", std::string{});
      if (!id.empty()) ids.push_back(id);
    }
  }
  std::string joined;
  for (std::size_t index = 0; index < ids.size() && index < 30; ++index) {
    if (!joined.empty()) joined += ", ";
    joined += ids[index];
  }
  if (!joined.empty()) fields_[4].value = joined;
  status_ = ids.empty() ? "provider advertised no models" : "discovered " + std::to_string(ids.size()) + " models";
  status_error_ = ids.empty();
  state.notice = status_;
}

void ProviderPage::save(ControlClient& control, AppState& state) {
  if (fields_.empty()) return;
  const std::string provider = trim(fields_[0].value);
  if (provider.empty()) {
    status_ = "provider name is required";
    status_error_ = true;
    return;
  }
  json params = {{"provider", provider}};
  const auto changed = [&](std::size_t index) {
    return index < fields_.size() && index < initial_.size() && fields_[index].value != initial_[index];
  };
  const auto include = [&](std::size_t index, const char* key, bool create_nonempty) {
    if (index >= fields_.size()) return;
    if (create_ ? (create_nonempty && !fields_[index].value.empty()) : changed(index)) {
      params[key] = fields_[index].value;
    }
  };
  include(1, "displayName", true);
  include(2, "api", true);
  include(3, "baseURL", true);
  if (4 < fields_.size()) {
    std::vector<std::string> models = split_models(fields_[4].value);
    if (create_ ? !models.empty() : fields_[4].value != initial_[4]) params["models"] = models;
  }
  if (5 < fields_.size() && !fields_[5].value.empty()) {
    if (fields_[5].value == "!clear") params["removeKey"] = true;
    else params["apiKey"] = fields_[5].value;
  }
  if (params.size() == 1) {
    status_ = "no changes";
    status_error_ = false;
    return;
  }
  const json result = request_with_retry(control, "providers.update", params, std::chrono::seconds(30));
  if (result.contains("__error")) {
    status_ = result["__error"].get<std::string>();
    status_error_ = true;
    return;
  }
  const std::string text = result.value("text", std::string{"provider updated"});
  state.notice = text;
  visible_ = false;
}

void ProviderPage::remove(ControlClient& control, AppState& state) {
  if (create_ || !configured_) {
    status_ = "this route has no user configuration to remove";
    status_error_ = true;
    return;
  }
  const json result = request_with_retry(control, "providers.remove",
                                         {{"provider", provider_}, {"removeKey", true}},
                                         std::chrono::seconds(30));
  if (result.contains("__error")) {
    status_ = result["__error"].get<std::string>();
    status_error_ = true;
    return;
  }
  state.notice = result.value("text", std::string{"provider removed"});
  visible_ = false;
}

bool ProviderPage::key(const ftxui::Event& event, ControlClient& control, AppState& state) {
  if (!visible_) return false;
  if (event == ftxui::Event::Escape) {
    close();
    return true;
  }
  if (event == ftxui::Event::Tab || event == ftxui::Event::ArrowDown) {
    move(1);
    return true;
  }
  if (event == ftxui::Event::TabReverse || event == ftxui::Event::ArrowUp) {
    move(-1);
    return true;
  }
  if (event == ftxui::Event::Backspace) {
    erase_last();
    return true;
  }
  if (event == ftxui::Event::Return) {
    if (last_editable()) save(control, state);
    else move(1);
    return true;
  }
  if (event == ftxui::Event::CtrlS) {
    save(control, state);
    return true;
  }
  if (event == ftxui::Event::CtrlX) {
    remove(control, state);
    return true;
  }
  if (event == ftxui::Event::CtrlF) {
    discover(control, state);
    return true;
  }
  if (event.is_character()) {
    if (fields_.empty() || fields_[active_].read_only) return true;
    const std::string ch = event.character();
    if (ch.size() == 1 && (static_cast<unsigned char>(ch[0]) < 0x20 || ch[0] == 0x7f)) return true;
    fields_[active_].value += ch;
    return true;
  }
  return true;
}

ftxui::Element ProviderPage::view() const {
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto panel = theme_color("panel", ftxui::Color::RGB(18, 18, 18));
  const auto hover = theme_color("hover", ftxui::Color::RGB(40, 40, 40));
  ftxui::Elements rows;
  const std::string title = create_ || provider_.empty() ? "new provider" : "provider · " + provider_;
  rows.push_back(ftxui::text(" " + title + " ") | ftxui::bold | ftxui::color(accent));
  if (!status_.empty()) {
    rows.push_back(ftxui::paragraph(status_) |
                   ftxui::color(status_error_ ? ftxui::Color::RGB(255, 95, 95) : dim));
  }
  rows.push_back(ftxui::text(""));
  for (std::size_t index = 0; index < fields_.size(); ++index) {
    const auto& field = fields_[index];
    const bool active = index == active_;
    std::string value = field.value;
    if (field.secret && !value.empty()) value = std::string(std::min<std::size_t>(value.size(), 24), '*');
    if (value.empty()) value = field.hint;
    ftxui::Element line = ftxui::hbox({
        ftxui::text((active ? " › " : "   ")) | ftxui::color(accent),
        ftxui::text(field.label + "  ") | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 14) | ftxui::color(dim),
        ftxui::text(value) | ftxui::color(field.read_only ? dim : (field.value.empty() ? dim : text)),
        ftxui::filler(),
    });
    if (active) line = line | ftxui::bgcolor(hover);
    rows.push_back(std::move(line));
  }
  rows.push_back(ftxui::text(""));
  rows.push_back(ftxui::text(" Tab/↑↓ field · Enter next/save · Ctrl+S save · Ctrl+F discover · Ctrl+X remove · Esc cancel") |
                 ftxui::dim);
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 76) |
         ftxui::borderRounded | ftxui::center | ftxui::bgcolor(panel);
}

}  // namespace booth

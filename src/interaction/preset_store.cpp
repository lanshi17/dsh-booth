#include "interaction/preset_store.hpp"

#include <fstream>
#include <utility>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using json = nlohmann::json;

json read_document(const std::filesystem::path& file) {
  std::ifstream input(file);
  if (!input) return json{{"presets", json::array()}};
  auto document = json::parse(input, nullptr, false);
  if (document.is_discarded() || !document.is_object() || !document.contains("presets") ||
      !document["presets"].is_array()) {
    return json{{"presets", json::array()}};
  }
  return document;
}

bool write_document(const std::filesystem::path& file, const json& document) {
  std::error_code error;
  std::filesystem::create_directories(file.parent_path(), error);
  auto temporary = file;
  temporary += ".tmp";
  std::ofstream output(temporary, std::ios::trunc);
  if (!output) return false;
  output << document.dump(2) << '\n';
  output.close();
  if (!output) return false;
  std::filesystem::rename(temporary, file, error);
  if (error) {
    std::filesystem::remove(temporary, error);
    return false;
  }
  std::filesystem::permissions(file,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
  return true;
}

ModelPreset parse(const json& row) {
  ModelPreset preset;
  if (!row.is_object()) return preset;
  preset.name = row.value("name", std::string{});
  preset.model_value = row.value("model", std::string{});
  preset.model_name = row.value("modelName", std::string{});
  preset.effort_id = row.value("effortId", std::string{});
  preset.effort_value = row.value("effort", std::string{});
  preset.effort_name = row.value("effortName", std::string{});
  return preset;
}

json encode(const ModelPreset& preset) {
  return {{"name", preset.name},
          {"model", preset.model_value},
          {"modelName", preset.model_name},
          {"effortId", preset.effort_id},
          {"effort", preset.effort_value},
          {"effortName", preset.effort_name}};
}

}  // namespace

std::vector<ModelPreset> load_model_presets(const std::filesystem::path& file) {
  std::vector<ModelPreset> presets;
  for (const auto& row : read_document(file)["presets"]) {
    auto preset = parse(row);
    if (!preset.name.empty()) presets.push_back(std::move(preset));
  }
  return presets;
}

bool save_model_preset(const std::filesystem::path& file, const ModelPreset& preset) {
  if (preset.name.empty()) return false;
  auto document = read_document(file);
  bool replaced = false;
  for (auto& row : document["presets"]) {
    if (row.is_object() && row.value("name", std::string{}) == preset.name) {
      row = encode(preset);
      replaced = true;
      break;
    }
  }
  if (!replaced) document["presets"].push_back(encode(preset));
  return write_document(file, document);
}

bool remove_model_preset(const std::filesystem::path& file, const std::string& name) {
  auto document = read_document(file);
  json kept = json::array();
  for (const auto& row : document["presets"]) {
    if (row.is_object() && row.value("name", std::string{}) == name) continue;
    kept.push_back(row);
  }
  if (kept.size() == document["presets"].size()) return false;
  document["presets"] = std::move(kept);
  return write_document(file, document);
}

std::optional<ModelPreset> find_model_preset(const std::filesystem::path& file,
                                             const std::string& name) {
  for (auto& preset : load_model_presets(file)) {
    if (preset.name == name) return preset;
  }
  return std::nullopt;
}

std::string model_preset_list(const std::filesystem::path& file) {
  const auto presets = load_model_presets(file);
  if (presets.empty()) return "no model presets · /modelpreset save <name>";
  std::string out = "model presets";
  for (const auto& preset : presets) {
    out += " · " + preset.name;
    if (!preset.model_name.empty()) out += " (" + preset.model_name;
    else if (!preset.model_value.empty()) out += " (" + preset.model_value;
    else out += " (";
    if (!preset.effort_name.empty()) out += "/" + preset.effort_name;
    out += ")";
  }
  return out;
}

}  // namespace booth

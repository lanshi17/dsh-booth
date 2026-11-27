#pragma once

#include <filesystem>
#include <optional>
#include <string>
#include <vector>

namespace booth {

struct ModelPreset {
  std::string name;
  std::string model_value;
  std::string model_name;
  std::string effort_id;
  std::string effort_value;
  std::string effort_name;
};

/// Reads presets; a missing or malformed file yields an empty list.
std::vector<ModelPreset> load_model_presets(const std::filesystem::path& file);

/// Inserts or replaces one preset by name.
bool save_model_preset(const std::filesystem::path& file, const ModelPreset& preset);

/// Removes one preset by name; false when absent.
bool remove_model_preset(const std::filesystem::path& file, const std::string& name);

std::optional<ModelPreset> find_model_preset(const std::filesystem::path& file,
                                             const std::string& name);

/// One notice line listing every preset.
std::string model_preset_list(const std::filesystem::path& file);

}  // namespace booth

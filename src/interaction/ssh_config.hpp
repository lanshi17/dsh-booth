#pragma once

#include <filesystem>
#include <string>
#include <string_view>

namespace booth {

/// OMP-compatible `/ssh` host registry: add/list/remove named JSON hosts.
std::string handle_ssh_command(std::string_view args, const std::filesystem::path& user_config,
                               const std::filesystem::path& project_config);

}  // namespace booth

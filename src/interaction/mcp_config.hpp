#pragma once

#include <filesystem>
#include <string>

#include <nlohmann/json.hpp>

namespace booth {

/// The stdio/HTTP MCP servers booth passes to `session/new` and `session/resume`.
nlohmann::json load_mcp_servers(const std::filesystem::path& file);

/// Persist one server definition under its unique name. Returns the stored row.
nlohmann::json upsert_mcp_server(const std::filesystem::path& file, std::string name,
                                 nlohmann::json server);

/// Remove one server by name; false when the name is absent.
bool remove_mcp_server(const std::filesystem::path& file, const std::string& name);

/// Enable or disable a stored server; disabled rows stay in the file but are
/// not passed to the next session. False when the name is absent.
bool set_mcp_enabled(const std::filesystem::path& file, const std::string& name, bool enabled);

/// Validate one stored server's transport fields for `/mcp test`.
std::string mcp_test_text(const std::filesystem::path& file, const std::string& name);

/// Render the configured servers for the notice line.
std::string mcp_list_text(const std::filesystem::path& file);

}  // namespace booth

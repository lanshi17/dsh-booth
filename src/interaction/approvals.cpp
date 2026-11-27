#include "interaction/approvals.hpp"

#include <filesystem>
#include <fstream>
#include <string>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using json = nlohmann::json;

std::string approval_key(const PermissionRequest& request) {
  return request.tool + '\x1f' + request.title + '\x1f' + request.detail;
}

bool read_grants(const std::string& path, json& document, std::string& error) {
  document = json::object();
  std::error_code ec;
  if (!std::filesystem::exists(path, ec)) return true;
  std::ifstream input(path);
  if (!input.is_open()) {
    error = "cannot read " + path;
    return false;
  }
  document = json::parse(input, nullptr, false);
  if (document.is_discarded() || !document.is_object()) {
    error = "invalid approval store " + path;
    return false;
  }
  return true;
}

bool write_grants(const std::string& path, const json& document, std::string& error) {
  std::error_code ec;
  const std::filesystem::path target(path);
  if (!target.parent_path().empty()) std::filesystem::create_directories(target.parent_path(), ec);
  if (ec) {
    error = ec.message();
    return false;
  }
  auto temp = target;
  temp += ".tmp";
  {
    std::ofstream output(temp, std::ios::out | std::ios::trunc | std::ios::binary);
    if (!output.is_open()) {
      error = "cannot write " + temp.string();
      return false;
    }
    output << document.dump(2) << '\n';
  }
  std::filesystem::permissions(temp, std::filesystem::perms::owner_read | std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, ec);
  std::filesystem::rename(temp, target, ec);
  if (ec) {
    error = "cannot replace " + target.string() + ": " + ec.message();
    return false;
  }
  return true;
}

}  // namespace

bool remember_approval(const std::string& path, const PermissionRequest& request, std::string& error) {
  if (path.empty()) {
    error = "approval store is not configured";
    return false;
  }
  json document;
  if (!read_grants(path, document, error)) return false;
  document[approval_key(request)] = true;
  return write_grants(path, document, error);
}

std::optional<std::string> remembered_approval(const std::string& path, const PermissionRequest& request) {
  if (path.empty()) return std::nullopt;
  json document;
  std::string error;
  if (!read_grants(path, document, error)) return std::nullopt;
  if (!document.contains(approval_key(request))) return std::nullopt;
  for (const auto& option : request.options) {
    if (option.kind == "allow_always" && !option.id.empty()) return option.id;
  }
  return std::nullopt;
}

bool clear_approvals(const std::string& path, std::string& error) {
  if (path.empty()) return true;
  std::error_code ec;
  std::filesystem::remove(path, ec);
  if (ec && ec != std::errc::no_such_file_or_directory) {
    error = ec.message();
    return false;
  }
  return true;
}

}  // namespace booth

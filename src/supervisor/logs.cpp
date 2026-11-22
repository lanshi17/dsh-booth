#include "supervisor/logs.hpp"

#include <fcntl.h>
#include <unistd.h>

#include <algorithm>
#include <cerrno>
#include <cstddef>
#include <string>
#include <vector>

namespace booth {
namespace {

bool write_all(int fd, std::string_view bytes) {
  while (!bytes.empty()) {
    const ssize_t wrote = ::write(fd, bytes.data(), bytes.size());
    if (wrote < 0) {
      if (errno == EINTR) continue;
      return false;
    }
    bytes.remove_prefix(static_cast<std::size_t>(wrote));
  }
  return true;
}

std::vector<std::string> split_lines(const std::string& text) {
  std::vector<std::string> out;
  std::size_t start = 0;
  while (start < text.size()) {
    const std::size_t end = text.find('\n', start);
    if (end == std::string::npos) break;
    if (end > start) out.push_back(text.substr(start, end - start));
    start = end + 1;
  }
  return out;
}

}  // namespace

Log::Log(std::filesystem::path path, std::size_t max_bytes)
    : path_(std::move(path)), max_bytes_(max_bytes == 0 ? 1 : max_bytes) {}

void Log::rotate_if_needed() {
  std::error_code error;
  const auto size = std::filesystem::file_size(path_, error);
  if (error || size < max_bytes_) return;
  auto rotated = path_;
  rotated += ".1";
  std::filesystem::remove(rotated, error);
  std::filesystem::rename(path_, rotated, error);
}

Result<void> Log::append(std::string_view line, std::string_view tag) {
  std::error_code error;
  std::filesystem::create_directories(path_.parent_path(), error);
  if (error) {
    return std::unexpected(Error{Layer::State, "cannot create the log directory", std::nullopt});
  }
  rotate_if_needed();

  const int fd = ::open(path_.c_str(), O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
  if (fd < 0) {
    return std::unexpected(Error{Layer::State, "cannot open the log file", std::nullopt});
  }
  std::string bytes;
  if (!tag.empty()) {
    bytes.append(tag);
    bytes.append(": ");
  }
  bytes.append(line);
  bytes.push_back('\n');
  const bool wrote = write_all(fd, bytes);
  ::close(fd);
  if (!wrote) {
    return std::unexpected(Error{Layer::State, "cannot write the log file", std::nullopt});
  }
  return {};
}

Result<std::vector<std::string>> Log::tail(std::size_t lines) const {
  std::vector<std::string> empty;
  const int fd = ::open(path_.c_str(), O_RDONLY | O_CLOEXEC);
  if (fd < 0) {
    if (errno == ENOENT) return empty;
    return std::unexpected(Error{Layer::State, "cannot open the log file", std::nullopt});
  }

  off_t end = ::lseek(fd, 0, SEEK_END);
  if (end < 0) {
    ::close(fd);
    return std::unexpected(Error{Layer::State, "cannot seek the log file", std::nullopt});
  }

  std::string buffer;
  constexpr std::size_t kWindow = 8192;
  while (end > 0) {
    const off_t take = static_cast<off_t>(std::min<std::size_t>(kWindow, static_cast<std::size_t>(end)));
    end -= take;
    if (::lseek(fd, end, SEEK_SET) < 0) {
      ::close(fd);
      return std::unexpected(Error{Layer::State, "cannot seek the log file", std::nullopt});
    }
    std::string chunk(static_cast<std::size_t>(take), '\0');
    std::size_t filled = 0;
    while (filled < chunk.size()) {
      const ssize_t got = ::read(fd, chunk.data() + filled, chunk.size() - filled);
      if (got <= 0) break;
      filled += static_cast<std::size_t>(got);
    }
    chunk.resize(filled);
    buffer.insert(0, chunk);
    const auto newlines = static_cast<std::size_t>(std::count(buffer.begin(), buffer.end(), '\n'));
    if (newlines > lines) break;
  }
  ::close(fd);

  auto all = split_lines(buffer);
  if (all.size() > lines) {
    all.erase(all.begin(), all.end() - static_cast<std::ptrdiff_t>(lines));
  }
  return all;
}

}  // namespace booth

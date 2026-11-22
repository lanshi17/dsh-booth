#pragma once

#include <condition_variable>
#include <deque>
#include <functional>
#include <mutex>
#include <thread>

namespace booth {

/// Runs posted jobs one at a time off the UI thread.
class Worker {
 public:
  Worker();
  ~Worker();
  Worker(const Worker&) = delete;
  Worker& operator=(const Worker&) = delete;

  void post(std::function<void()> job);
  void stop();

 private:
  void loop();

  std::mutex mutex_;
  std::condition_variable ready_;
  std::deque<std::function<void()>> jobs_;
  bool stopping_{false};
  std::thread thread_;
};

}  // namespace booth

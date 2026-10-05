cmake_minimum_required(VERSION 3.20)
project(Greeting LANGUAGES CXX)

# The demo program.
set(CMAKE_CXX_STANDARD 20)
add_executable(greeting main.cpp)
if(WIN32)
  target_compile_definitions(greeting PRIVATE UNICODE)
endif()
message(STATUS "Building ${PROJECT_NAME}")

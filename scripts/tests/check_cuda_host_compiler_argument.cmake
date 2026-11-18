# SPDX-License-Identifier: Apache-2.0
# Exercise the UNIX_COMMAND parsing used by CMakeDetermineCompilerId.cmake.
if(NOT DEFINED HOST_COMPILER)
  message(FATAL_ERROR "HOST_COMPILER is required")
endif()
set(flags " -ccbin=\"${HOST_COMPILER}\"")
separate_arguments(arguments UNIX_COMMAND "${flags}")
list(LENGTH arguments count)
if(NOT count EQUAL 1 OR NOT arguments STREQUAL "-ccbin=${HOST_COMPILER}")
  message(FATAL_ERROR "CUDA host compiler argument changed during CMake parsing: ${arguments}")
endif()
message(STATUS "CUDA host compiler path round-tripped")

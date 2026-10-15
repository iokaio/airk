// SPDX-License-Identifier: Apache-2.0
plugins { id("fictional.local") }
dependencies {
    implementation("fictional:archive:1.2")
    testImplementation(project(":checks"))
}
tasks.register("inspectCafé") { println("written task") }
configurations.create("archiveTools")
include(":shared")
fun localCount(): Int { return 7 }

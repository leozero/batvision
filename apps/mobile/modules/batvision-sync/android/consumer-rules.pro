# WorkManager's Room database is created by reflection: R8 must keep its constructor.
-keep class * extends androidx.room.RoomDatabase { <init>(); }
-keep class androidx.work.impl.WorkDatabase_Impl { *; }
# The worker is instantiated by class name.
-keep class fr.batvision.sync.SyncWorker { <init>(...); }

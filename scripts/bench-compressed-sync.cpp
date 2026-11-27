#include <lz4.h>
#include <fcntl.h>
#include <unistd.h>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <vector>
#include <algorithm>
#include <iterator>
int main(int argc,char** argv){
 for(int arg=1;arg<argc;++arg){
  std::ifstream f(argv[arg],std::ios::binary);
  std::vector<char> src{std::istreambuf_iterator<char>(f),{}},dst(LZ4_compressBound(src.size()));
  if(src.empty())return 5;
  std::vector<char> decoded(src.size());
  std::vector<double> times[2],diff;
  for(int i=0;i<24;++i){
   for(int k=0;k<2;++k){
    int mode=(k+i)%2;
    char path[]="/tmp/lin-sync-XXXXXX";int fd=mkstemp(path);if(fd<0)return 1;
    const char *data=src.data();size_t size=src.size();
    auto start=std::chrono::steady_clock::now();
    if(mode){size=LZ4_compress_default(data,dst.data(),size,dst.size());if(!size)return 2;data=dst.data();}
    for(size_t pos=0;pos<size;){auto n=write(fd,data+pos,size-pos);if(n<=0)return 3;pos+=n;}
    if(fdatasync(fd)<0)return 4;
    times[mode].push_back(std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count());
    if(mode && (LZ4_decompress_safe(dst.data(),decoded.data(),size,decoded.size())!=static_cast<int>(src.size()) || decoded!=src))return 6;
    close(fd);unlink(path);
   }
   diff.push_back(times[1].back()-times[0].back());
  }
  auto median=[](std::vector<double> v){std::sort(v.begin(),v.end());return (v[11]+v[12])/2;};
  std::cout<<argv[arg]<<" raw "<<median(times[0])<<" lz4 "<<median(times[1])<<" paired "<<median(diff)<<" wins "<<std::count_if(diff.begin(),diff.end(),[](double x){return x<0;})<<"/24\n";
 }
}

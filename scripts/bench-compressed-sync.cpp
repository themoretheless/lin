#include <lz4.h>
#include <fcntl.h>
#include <unistd.h>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iostream>
#include <vector>
#include <algorithm>
#include <iterator>
int main(int argc,char** argv){
 for(int arg=1;arg<argc;++arg){
  std::ifstream f(argv[arg],std::ios::binary);
  std::vector<char> src{std::istreambuf_iterator<char>(f),{}},dst(LZ4_compressBound(src.size()));
  if(src.size()<8 || std::memcmp(src.data(),"LIN",3)!=0)return 5;
  std::uint32_t frame_length=0;
  for(unsigned i=0;i<4;++i)
   frame_length |= static_cast<std::uint32_t>(static_cast<unsigned char>(src[4+i]))<<(8*i);
  const std::size_t prefix=8ULL+frame_length;
  if(prefix>=src.size() || src.size()-prefix<8 || std::memcmp(src.data()+prefix,"LIN",3)!=0)return 5;
  std::vector<char> decoded(src.size()-prefix);
  std::vector<double> times[2],diff;
  for(int i=0;i<24;++i){
   for(int k=0;k<2;++k){
    int mode=(k+i)%2;
    char path[]="/tmp/lin-sync-XXXXXX";int fd=mkstemp(path);if(fd<0)return 1;
    if(fallocate(fd,FALLOC_FL_KEEP_SIZE,0,1048576)<0)return 7;
    if(write(fd,src.data(),prefix)!=static_cast<ssize_t>(prefix) || fdatasync(fd)<0)return 8;
    const char *data=src.data()+prefix;size_t size=src.size()-prefix;
    auto start=std::chrono::steady_clock::now();
    if(mode){size=LZ4_compress_default(data,dst.data(),size,dst.size());if(!size)return 2;data=dst.data();}
    if(prefix+size>1048576){
     const auto extent=((prefix+size+1048575)/1048576)*1048576;
     if(fallocate(fd,FALLOC_FL_KEEP_SIZE,0,extent)<0)return 9;
    }
    for(size_t pos=0;pos<size;){auto n=write(fd,data+pos,size-pos);if(n<=0)return 3;pos+=n;}
    if(fdatasync(fd)<0)return 4;
    times[mode].push_back(std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count());
    if(mode && (LZ4_decompress_safe(dst.data(),decoded.data(),size,decoded.size())!=static_cast<int>(decoded.size()) || !std::equal(decoded.begin(),decoded.end(),src.begin()+prefix)))return 6;
    close(fd);unlink(path);
   }
   diff.push_back(times[1].back()-times[0].back());
  }
  auto median=[](std::vector<double> v){std::sort(v.begin(),v.end());return (v[11]+v[12])/2;};
  std::cout<<argv[arg]<<" raw "<<median(times[0])<<" lz4 "<<median(times[1])<<" paired "<<median(diff)<<" wins "<<std::count_if(diff.begin(),diff.end(),[](double x){return x<0;})<<"/24\n";
 }
}
